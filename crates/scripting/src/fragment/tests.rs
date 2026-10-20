//! Runtime tests for quest-stage fragment dispatch — `apply_effects`
//! against the canonical stores, and `quest_fragment_dispatch_system`
//! end-to-end (marker → fragment → state, including the SetStage cascade).

use super::*;
use crate::papyrus_demo::PapyrusPlayerEntity;
use crate::quest_stages::{
    QuestFormId, QuestObjectiveState, QuestStageAdvanced, QuestStageAdvancedBatch, QuestStageState,
};
use crate::translate::compose::{ObjectRef, QuestRef};
use crate::translate::effects::{Effect, FragmentGuard};
use crate::CinematicAnimationEvent;
use byroredux_core::ecs::world::World;

const Q: QuestFormId = QuestFormId(0x0001_2345);

#[test]
fn copied_transform_releases_each_component_guard() {
    use byroredux_core::ecs::components::Transform;
    use byroredux_core::math::Vec3;

    let mut world = World::new();
    let first = world.spawn();
    let second = world.spawn();
    world.insert(first, Transform::from_translation(Vec3::X));
    world.insert(second, Transform::from_translation(Vec3::Y));

    let first_value = copied_transform(&world, first).expect("first transform");
    let second_value = copied_transform(&world, second).expect("second transform");
    let _write_after_reads = world.query_mut::<Transform>().expect("Transform storage");

    assert_eq!(first_value.translation, Vec3::X);
    assert_eq!(second_value.translation, Vec3::Y);
}

/// A world with the scripting subsystem registered, a player entity, and
/// a fresh quest-stage store — the standard fixture for dispatch tests.
fn fixture() -> World {
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
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
    let player = world.resource::<PapyrusPlayerEntity>().0;
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
                    stage_set: true,
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
    let mut deferred = DeferredFragmentEffects::new(&world);
    let advances = apply_effects(
        &effects,
        Q,
        None,
        &world,
        &mut stages,
        &mut objectives,
        &mut deferred,
    );
    deferred.apply(&world);

    assert_eq!(stages.get_stage(Q), 30);
    assert!(objectives.get(Q, 10).completed);
    assert!(objectives.get(Q, 20).displayed);
    // Only the SetStage produces a QuestStageAdvanced.
    assert_eq!(advances.len(), 1);
    assert_eq!(advances[0].new_stage, 30);
}

#[test]
fn provider_barrier_resumes_native_fragment_tail_after_host_call() {
    use std::sync::{Arc, Mutex};

    use crate::translate::effects::FragmentProviderCall;
    use byroredux_sdk::script_function::ScriptValue;

    let world = fixture();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let calls_for_callback = Arc::clone(&calls);
    let callback = Arc::new(
        move |_principal: Option<&byroredux_sdk::identity::PrincipalId>,
              route: &str,
              arguments: &[ScriptValue]| {
            calls_for_callback
                .lock()
                .unwrap()
                .push((route.to_owned(), arguments.to_vec()));
            Ok(ScriptValue::Integer(2))
        },
    ) as Arc<crate::PapyrusProviderCallback>;
    crate::set_papyrus_provider_runtime(
        &world,
        Arc::new(crate::PapyrusProviderCatalog::default()),
        Some(callback),
    );
    let effects = [
        Effect::ProviderCall(FragmentProviderCall {
            route: "byro.content.catalog.get-mod-count".to_owned(),
            arguments: Vec::new(),
            principal: None,
        }),
        Effect::SetStage {
            quest: QuestRef::SelfRef,
            stage: 20,
        },
        Effect::ProviderCall(FragmentProviderCall {
            route: "byro.content.catalog.is-plugin-installed".to_owned(),
            arguments: vec![ScriptValue::String("Update.esm".to_owned())],
            principal: None,
        }),
        Effect::SetObjectiveCompleted {
            quest: QuestRef::SelfRef,
            objective: 5,
            completed: true,
        },
    ];
    let mut stages = QuestStageState::default();
    let mut objectives = QuestObjectiveState::default();
    let mut deferred = DeferredFragmentEffects::new(&world);

    apply_effects(
        &effects,
        Q,
        None,
        &world,
        &mut stages,
        &mut objectives,
        &mut deferred,
    );
    assert!(calls.lock().unwrap().is_empty());
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 0);

    let advances = deferred.apply(&world);
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &[
            ("byro.content.catalog.get-mod-count".to_owned(), Vec::new()),
            (
                "byro.content.catalog.is-plugin-installed".to_owned(),
                vec![ScriptValue::String("Update.esm".to_owned())],
            ),
        ]
    );
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
    assert!(world.resource::<QuestObjectiveState>().get(Q, 5).completed);
    assert_eq!(advances.len(), 1);
}

#[test]
fn branch_provider_barrier_resumes_branch_and_outer_tails_in_order() {
    use std::sync::{Arc, Mutex};

    use crate::translate::effects::FragmentProviderCall;
    use byroredux_sdk::script_function::ScriptValue;

    let world = fixture();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let calls_for_callback = Arc::clone(&calls);
    let callback = Arc::new(
        move |_principal: Option<&byroredux_sdk::identity::PrincipalId>,
              route: &str,
              _arguments: &[ScriptValue]| {
            calls_for_callback.lock().unwrap().push(route.to_owned());
            Ok(ScriptValue::Integer(1))
        },
    ) as Arc<crate::PapyrusProviderCallback>;
    crate::set_papyrus_provider_runtime(
        &world,
        Arc::new(crate::PapyrusProviderCatalog::default()),
        Some(callback),
    );
    let effects = [
        Effect::Conditional {
            guards: vec![FragmentGuard::StageDone {
                quest: QuestRef::SelfRef,
                stage: 2,
                done: false,
            }],
            then_effects: vec![
                Effect::ProviderCall(FragmentProviderCall {
                    route: "ext.example.branch".to_owned(),
                    arguments: Vec::new(),
                    principal: None,
                }),
                Effect::SetStage {
                    quest: QuestRef::SelfRef,
                    stage: 10,
                },
            ],
            else_effects: vec![Effect::SetStage {
                quest: QuestRef::SelfRef,
                stage: 99,
            }],
        },
        Effect::ProviderCall(FragmentProviderCall {
            route: "ext.example.outer".to_owned(),
            arguments: Vec::new(),
            principal: None,
        }),
        Effect::SetStage {
            quest: QuestRef::SelfRef,
            stage: 20,
        },
    ];
    let mut stages = QuestStageState::default();
    let mut objectives = QuestObjectiveState::default();
    let mut deferred = DeferredFragmentEffects::new(&world);

    let immediate = apply_effects(
        &effects,
        Q,
        None,
        &world,
        &mut stages,
        &mut objectives,
        &mut deferred,
    );
    assert!(immediate.is_empty());
    assert_eq!(stages.get_stage(Q), 0);
    assert!(calls.lock().unwrap().is_empty());

    let advances = deferred.apply(&world);
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &[
            "ext.example.branch".to_owned(),
            "ext.example.outer".to_owned()
        ]
    );
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
    assert_eq!(advances.len(), 2);
}

#[test]
fn failed_provider_barrier_aborts_its_native_fragment_tail() {
    use std::sync::Arc;

    use crate::translate::effects::FragmentProviderCall;

    let world = fixture();
    let callback = Arc::new(
        |_principal: Option<&byroredux_sdk::identity::PrincipalId>,
         _route: &str,
         _arguments: &[byroredux_sdk::script_function::ScriptValue]| {
            Err("provider failed".to_owned())
        },
    ) as Arc<crate::PapyrusProviderCallback>;
    crate::set_papyrus_provider_runtime(
        &world,
        Arc::new(crate::PapyrusProviderCatalog::default()),
        Some(callback),
    );
    let effects = [
        Effect::ProviderCall(FragmentProviderCall {
            route: "ext.example.fail".to_owned(),
            arguments: Vec::new(),
            principal: None,
        }),
        Effect::SetStage {
            quest: QuestRef::SelfRef,
            stage: 20,
        },
    ];
    let mut stages = QuestStageState::default();
    let mut objectives = QuestObjectiveState::default();
    let mut deferred = DeferredFragmentEffects::new(&world);

    apply_effects(
        &effects,
        Q,
        None,
        &world,
        &mut stages,
        &mut objectives,
        &mut deferred,
    );
    assert!(deferred.apply(&world).is_empty());
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 0);
}

/// #3946 — at the barrier cap, the deepest tail must be declined
/// *whole*, not applied-then-abandoned.
///
/// Each `ProviderCall` suspends the remainder of the effect list as a
/// tail, so a `[call, SetStage(n)]` pattern gives one barrier per level
/// with a committing effect at the head of each tail. That shape is what
/// makes the bug observable: the old cap was tested on entry to
/// `apply_at_depth` and returned before the four non-provider flushes,
/// but the tail that filled the arriving `deferred` had *already*
/// committed its `stages`/`objectives` writes under the caller's guards.
/// So crossing the cap advanced the quest one stage further while
/// silently discarding the activation / scene-binding /
/// reference-enable / cinematic effects queued beside it.
///
/// A tail whose first effect is the *next* provider call cannot
/// distinguish the two versions — nothing commits at that level either
/// way — which is exactly the trap this test was rewritten to escape.
#[test]
fn provider_barrier_cap_declines_the_deepest_tail_whole() {
    use std::sync::{Arc, Mutex};

    use crate::translate::effects::FragmentProviderCall;
    use byroredux_sdk::script_function::ScriptValue;

    let world = fixture();
    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_callback = Arc::clone(&calls);
    let callback = Arc::new(
        move |_principal: Option<&byroredux_sdk::identity::PrincipalId>,
              _route: &str,
              _arguments: &[ScriptValue]| {
            *calls_for_callback.lock().unwrap() += 1;
            Ok(ScriptValue::Integer(0))
        },
    ) as Arc<crate::PapyrusProviderCallback>;
    crate::set_papyrus_provider_runtime(
        &world,
        Arc::new(crate::PapyrusProviderCatalog::default()),
        Some(callback),
    );

    // `[call, SetStage(1), call, SetStage(2), …]` — the tail entered at
    // depth d commits SetStage(d + 1) before suspending on the next call.
    // Built past the cap so the boundary, not the list length, is what
    // stops it.
    let levels = MAX_PROVIDER_FRAGMENT_BARRIERS + 8;
    let mut effects: Vec<Effect> = Vec::with_capacity(levels * 2);
    for level in 1..=levels {
        effects.push(Effect::ProviderCall(FragmentProviderCall {
            route: "ext.example.barrier".to_owned(),
            arguments: Vec::new(),
            principal: None,
        }));
        effects.push(Effect::SetStage {
            quest: QuestRef::SelfRef,
            stage: level as u16,
        });
    }

    let mut stages = QuestStageState::default();
    let mut objectives = QuestObjectiveState::default();
    let mut deferred = DeferredFragmentEffects::new(&world);
    apply_effects(
        &effects,
        Q,
        None,
        &world,
        &mut stages,
        &mut objectives,
        &mut deferred,
    );
    deferred.apply(&world);

    assert_eq!(
        *calls.lock().unwrap(),
        MAX_PROVIDER_FRAGMENT_BARRIERS,
        "the cap bounds how many provider barriers are crossed"
    );
    // Recursion is allowed while `depth + 1 < MAX`, so the deepest tail
    // actually entered is at depth MAX - 2 and commits stage MAX - 1.
    // The pre-#3946 code entered one level further and committed stage
    // MAX before discarding that level's deferred effects — so this exact
    // number is what separates the two.
    assert_eq!(
        world.resource::<QuestStageState>().get_stage(Q),
        (MAX_PROVIDER_FRAGMENT_BARRIERS - 1) as u16,
        "the tail at the cap must not be applied at all; committing its \
         stage while dropping its queued deferred effects is the partial \
         application #3946 removed"
    );
}

#[test]
fn quest_fragments_flush_provider_barriers_before_the_next_event() {
    use std::sync::Arc;

    use crate::translate::effects::FragmentProviderCall;
    use byroredux_sdk::script_function::ScriptValue;

    const Q2: QuestFormId = QuestFormId(0x0001_2346);

    let world = fixture();
    let callback = Arc::new(
        |_principal: Option<&byroredux_sdk::identity::PrincipalId>,
         _route: &str,
         _arguments: &[ScriptValue]| Ok(ScriptValue::Integer(1)),
    ) as Arc<crate::PapyrusProviderCallback>;
    crate::set_papyrus_provider_runtime(
        &world,
        Arc::new(crate::PapyrusProviderCatalog::default()),
        Some(callback),
    );
    {
        let mut fragments = world.resource_mut::<QuestStageFragments>();
        fragments.insert(
            Q,
            1,
            vec![
                Effect::ProviderCall(FragmentProviderCall {
                    route: "ext.example.first-quest".to_owned(),
                    arguments: Vec::new(),
                    principal: None,
                }),
                Effect::SetStage {
                    quest: QuestRef::SelfRef,
                    stage: 10,
                },
            ],
        );
        fragments.insert(
            Q2,
            1,
            vec![Effect::SetStage {
                quest: QuestRef::SelfRef,
                stage: 20,
            }],
        );
    }
    emit_advances(&world, &[(Q, 1), (Q2, 1)]);

    quest_fragment_dispatch_system(&world);

    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 10);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q2), 20);
    let player = world.resource::<PapyrusPlayerEntity>().0;
    let query = world.query::<QuestStageAdvancedBatch>().unwrap();
    let batch = query
        .get(player)
        .expect("quest SetStage compatibility batch");
    assert_eq!(
        batch
            .0
            .iter()
            .filter_map(|advance| (advance.new_stage >= 10).then_some(advance.new_stage))
            .collect::<Vec<_>>(),
        vec![10, 20]
    );
}

#[test]
fn apply_effects_selects_get_stage_done_conditional_branch() {
    let world = World::new();
    let effects = vec![Effect::Conditional {
        guards: vec![FragmentGuard::StageDone {
            quest: QuestRef::SelfRef,
            stage: 2,
            done: false,
        }],
        then_effects: vec![Effect::SetStage {
            quest: QuestRef::SelfRef,
            stage: 35,
        }],
        else_effects: vec![Effect::SetStage {
            quest: QuestRef::SelfRef,
            stage: 7,
        }],
    }];

    for (stage_2_done, expected) in [(false, 35), (true, 7)] {
        let mut stages = QuestStageState::default();
        if stage_2_done {
            stages.set_stage(Q, 2);
        }
        let mut objectives = QuestObjectiveState::default();
        let mut deferred = DeferredFragmentEffects::new(&world);
        let advances = apply_effects(
            &effects,
            Q,
            None,
            &world,
            &mut stages,
            &mut objectives,
            &mut deferred,
        );
        assert_eq!(advances.len(), 1);
        assert_eq!(advances[0].new_stage, expected);
        assert!(stages.get_stage_done(Q, expected));
    }
}

/// #3785 — a `Conditional` guard whose quest ref can't be resolved
/// (`QuestRef::Property` with no VMAD to look it up in, the documented
/// exposure) must decline the WHOLE `Conditional`, running neither
/// `then_effects` nor `else_effects` — not silently fall into
/// `else_effects` the way `is_some_and` used to collapse "false" and
/// "unresolvable" into the same outcome. Both branches here would
/// produce an observable `QuestStageAdvanced` if run, so a `0`-length
/// `advances` and both stages staying undone together prove neither ran.
#[test]
fn apply_effects_declines_conditional_with_unresolvable_guard() {
    let world = World::new();
    let effects = vec![Effect::Conditional {
        guards: vec![FragmentGuard::StageDone {
            quest: QuestRef::Property("SomeQuestProperty".to_owned()),
            stage: 2,
            done: false,
        }],
        then_effects: vec![Effect::SetStage {
            quest: QuestRef::SelfRef,
            stage: 35,
        }],
        else_effects: vec![Effect::SetStage {
            quest: QuestRef::SelfRef,
            stage: 7,
        }],
    }];

    let mut stages = QuestStageState::default();
    let mut objectives = QuestObjectiveState::default();
    let mut deferred = DeferredFragmentEffects::new(&world);
    let advances = apply_effects(
        &effects,
        Q,
        // No VMAD to resolve `QuestRef::Property` against — the guard is
        // unresolvable, matching the alias-bound / absent-property
        // exposure the issue documents.
        None,
        &world,
        &mut stages,
        &mut objectives,
        &mut deferred,
    );
    assert!(
        advances.is_empty(),
        "an unresolvable guard must run neither branch, got {advances:?}"
    );
    assert!(!stages.get_stage_done(Q, 35), "then_effects must not run");
    assert!(!stages.get_stage_done(Q, 7), "else_effects must not run");
}

#[test]
fn apply_effects_runs_quest_lifecycle_and_resets_objectives() {
    let mut world = World::new();
    crate::register(&mut world);
    crate::install_start_game_quests(
        &mut world,
        [byroredux_plugin::esm::records::QustRecord {
            form_id: Q.0,
            start_up_stage: Some(5),
            shut_down_stage: Some(90),
            objectives: vec![
                byroredux_plugin::esm::records::QuestObjective {
                    index: 10,
                    ..Default::default()
                },
                byroredux_plugin::esm::records::QuestObjective {
                    index: 20,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
    );
    let mut stages = QuestStageState::default();
    let mut objectives = QuestObjectiveState::default();
    let effects = vec![
        Effect::StartQuest {
            quest: QuestRef::SelfRef,
        },
        Effect::SetQuestActive {
            quest: QuestRef::SelfRef,
            active: true,
        },
        Effect::CompleteAllObjectives {
            quest: QuestRef::SelfRef,
        },
        Effect::StopQuest {
            quest: QuestRef::SelfRef,
        },
    ];
    let mut deferred = DeferredFragmentEffects::new(&world);
    let advances = apply_effects(
        &effects,
        Q,
        None,
        &world,
        &mut stages,
        &mut objectives,
        &mut deferred,
    );
    deferred.apply(&world);
    assert_eq!(advances.len(), 2);
    assert_eq!(advances[0].new_stage, 5);
    assert_eq!(advances[1].new_stage, 90);
    assert_eq!(stages.get_stage(Q), 90);
    assert!(stages.get_stage_done(Q, 90));
    assert!(!stages.is_running(Q));
    assert!(!stages.is_active(Q));
    assert!(objectives.get(Q, 10).completed);
    assert!(objectives.get(Q, 20).completed);

    let mut deferred = DeferredFragmentEffects::new(&world);
    apply_effects(
        &[Effect::ResetQuest {
            quest: QuestRef::SelfRef,
        }],
        Q,
        None,
        &world,
        &mut stages,
        &mut objectives,
        &mut deferred,
    );
    deferred.apply(&world);
    assert!(!stages.is_started(Q));
    assert_eq!(objectives.get(Q, 10), Default::default());
}

#[test]
fn set_stage_honors_the_authored_allow_repeated_stages_flag() {
    use byroredux_plugin::esm::records::QUEST_FLAG_ALLOW_REPEATED_STAGES;

    for (flags, expected_advances) in [(0, 0), (QUEST_FLAG_ALLOW_REPEATED_STAGES, 1)] {
        let mut world = World::new();
        crate::register(&mut world);
        crate::install_start_game_quests(
            &mut world,
            [byroredux_plugin::esm::records::QustRecord {
                form_id: Q.0,
                quest_flags: flags,
                ..Default::default()
            }],
        );
        let mut stages = QuestStageState::default();
        stages.set_stage(Q, 10);
        let mut objectives = QuestObjectiveState::default();
        let mut deferred = DeferredFragmentEffects::new(&world);
        let advances = apply_effects(
            &[Effect::SetStage {
                quest: QuestRef::SelfRef,
                stage: 10,
            }],
            Q,
            None,
            &world,
            &mut stages,
            &mut objectives,
            &mut deferred,
        );
        deferred.apply(&world);
        assert_eq!(advances.len(), expected_advances);
    }
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
    let mut deferred = DeferredFragmentEffects::new(&world);
    let advances = apply_effects(
        &effects,
        Q,
        None,
        &world,
        &mut stages,
        &mut objectives,
        &mut deferred,
    );
    deferred.apply(&world);
    assert!(advances.is_empty());
    assert_eq!(stages.get_stage(Q), 0, "no quest was touched");
}

#[test]
fn set_global_value_resolves_vmad_and_updates_runtime_table() {
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    const GAME_HOUR: u32 = 0x0000_0038;
    let mut world = World::new();
    world.insert_resource(crate::Globals::default());
    let vmad = ScriptInstanceData {
        scripts: vec![ScriptInstance {
            name: "QF_GlobalValueTest".into(),
            status: 0,
            properties: vec![ScriptProperty {
                name: "GameHour".into(),
                status: 1,
                value: PropertyValue::Object {
                    form_id: GAME_HOUR,
                    alias: -1,
                },
            }],
        }],
        ..Default::default()
    };
    let mut stages = QuestStageState::default();
    let mut objectives = QuestObjectiveState::default();
    let mut deferred = DeferredFragmentEffects::new(&world);
    let advances = apply_effects(
        &[Effect::SetGlobalValue {
            global: ObjectRef::Property("::GameHour_var".into()),
            value: crate::translate::effects::ValueExpr::Literal(7.0),
        }],
        Q,
        Some(&vmad),
        &world,
        &mut stages,
        &mut objectives,
        &mut deferred,
    );

    assert!(advances.is_empty());
    assert_eq!(world.resource::<crate::Globals>().get(GAME_HOUR), Some(7.0));
}

/// Regression for #2269: quest fragment dispatch must not acquire
/// `CinematicPresentationState` while the paired quest-state guards are held.
/// Both affected presentation operations stay queued until the guard scope
/// ends, then apply in authored order.
#[test]
fn cinematic_presentation_effects_wait_for_quest_guards_to_drop() {
    let world = fixture();
    let effects = [
        Effect::SetSittingRotation { degrees: -55.0 },
        Effect::RegisterPlayerAnimationEvent {
            event: CinematicAnimationEvent::IdleFurnitureExit,
        },
    ];
    let mut deferred = DeferredFragmentEffects::new(&world);

    let advances = {
        let (mut stages, mut objectives) =
            world.resource_2_mut::<QuestStageState, QuestObjectiveState>();
        apply_effects(
            &effects,
            Q,
            None,
            &world,
            &mut stages,
            &mut objectives,
            &mut deferred,
        )
    };
    assert!(advances.is_empty());

    {
        let presentation = world.resource::<crate::CinematicPresentationState>();
        assert_eq!(presentation.sitting_rotation_degrees, 0.0);
        assert!(!presentation
            .is_player_animation_event_registered(CinematicAnimationEvent::IdleFurnitureExit));
    }

    deferred.apply(&world);
    let presentation = world.resource::<crate::CinematicPresentationState>();
    assert_eq!(presentation.sitting_rotation_degrees, -55.0);
    assert!(presentation
        .is_player_animation_event_registered(CinematicAnimationEvent::IdleFurnitureExit));
}

/// MQ101's execution-block chargen gate (stages 65/75/80): `SetInChargen`,
/// `ShowRaceMenu`, and `RequestSave`/`RequestAutoSave` all apply through the
/// same deferred `CinematicPresentationState` path as the effects above,
/// rather than declining the fragments they're authored in.
#[test]
fn chargen_effects_apply_through_deferred_cinematic_presentation() {
    let world = fixture();
    let effects = [
        Effect::SetInChargen {
            disable_saving: false,
            disable_waiting: true,
            show_controls_disabled_message: true,
        },
        Effect::ShowRaceMenu,
        Effect::RequestSave { auto: false },
        Effect::RequestSave { auto: true },
    ];
    let mut deferred = DeferredFragmentEffects::new(&world);

    let advances = {
        let (mut stages, mut objectives) =
            world.resource_2_mut::<QuestStageState, QuestObjectiveState>();
        apply_effects(
            &effects,
            Q,
            None,
            &world,
            &mut stages,
            &mut objectives,
            &mut deferred,
        )
    };
    assert!(advances.is_empty());

    // Not applied until the guard scope drops, same as the sibling test above.
    {
        let presentation = world.resource::<crate::CinematicPresentationState>();
        assert!(!presentation.disable_waiting);
        assert_eq!(presentation.race_menu_shown_count, 0);
        assert_eq!(presentation.save_requested_count, 0);
    }

    deferred.apply(&world);
    let presentation = world.resource::<crate::CinematicPresentationState>();
    assert!(!presentation.disable_saving);
    assert!(presentation.disable_waiting);
    assert!(presentation.show_controls_disabled_message);
    assert_eq!(presentation.race_menu_shown_count, 1);
    assert_eq!(presentation.save_requested_count, 2);
    assert!(presentation.last_save_was_auto);
}

/// MQ101's dragon-attack/keep-escape combat gate (stages 270+):
/// `Faction.SetEnemy` resolves both `Faction` VMAD properties and records
/// each direction's reaction in `FactionRelations`, keyed by portable
/// identity (#4414); `Actor.StartCombat` resolves both
/// the acting alias and the player, then arms `AiCombatState` on the
/// actor. Both go through `DeferredFragmentEffects` — `SetEnemy` because
/// `FactionRelations` is a resource (the #2269 nested-acquisition
/// caution), `StartCombat` applies immediately like `EvaluatePackage`
/// because `AiCombatState` is per-entity component storage.
#[test]
fn combat_gate_effects_set_faction_hostility_and_arm_ai_combat_state() {
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    const STORMCLOAK_FACTION: u32 = 0x0009_9999;
    const PLAYER_FACTION: u32 = 0x0000_0013;
    const ATTACKER_ALIAS: i16 = 5;

    let mut world = fixture();
    let skyrim = byroredux_core::form_id::PluginId::from_filename("Skyrim.esm");
    let identity = crate::LoadOrderIdentity::new(vec![(
        byroredux_plugin::esm::reader::GlobalSlot::Regular(0),
        skyrim,
    )]);
    let stormcloaks = identity.form_ref(STORMCLOAK_FACTION).unwrap();
    let player_faction = identity.form_ref(PLAYER_FACTION).unwrap();
    world.insert_resource(identity);
    let attacker = world.spawn();
    world
        .resource_mut::<crate::SceneActorBindings>()
        .bind(Q, i32::from(ATTACKER_ALIAS), attacker);
    // #5046 — the ambient faction pass already armed this actor against the
    // same target the fragment names (a hostile-faction ambush actor).
    let player_before = world.resource::<PapyrusPlayerEntity>().0;
    world.insert(
        attacker,
        crate::AmbientEngagement {
            target: player_before,
            out_of_contact_secs: 4.0,
        },
    );
    let vmad = ScriptInstanceData {
        scripts: vec![ScriptInstance {
            name: "MQ101QuestScript".into(),
            status: 0,
            properties: vec![
                ScriptProperty {
                    name: "MQ101StormcloakFaction".into(),
                    status: 1,
                    value: PropertyValue::Object {
                        form_id: STORMCLOAK_FACTION,
                        alias: -1,
                    },
                },
                ScriptProperty {
                    name: "PlayerFaction".into(),
                    status: 1,
                    value: PropertyValue::Object {
                        form_id: PLAYER_FACTION,
                        alias: -1,
                    },
                },
                ScriptProperty {
                    name: "Alias_TrophyRoomPrisoner01".into(),
                    status: 1,
                    value: PropertyValue::Object {
                        form_id: 0,
                        alias: ATTACKER_ALIAS,
                    },
                },
            ],
        }],
        ..Default::default()
    };

    let effects = [
        // `abOtherIsNeutralToSelf` set: only the reverse direction is
        // neutral, which the old undirected pair could not express.
        Effect::SetEnemy {
            faction: ObjectRef::Property("MQ101StormcloakFaction".into()),
            other_faction: ObjectRef::Property("PlayerFaction".into()),
            self_neutral: false,
            other_neutral: true,
        },
        Effect::StartCombat {
            actor: crate::translate::effects::ActorRef::Object(ObjectRef::Property(
                "Alias_TrophyRoomPrisoner01".into(),
            )),
            target: crate::translate::effects::ActorRef::Player,
        },
    ];
    let mut deferred = DeferredFragmentEffects::new(&world);
    let advances = {
        let (mut stages, mut objectives) =
            world.resource_2_mut::<QuestStageState, QuestObjectiveState>();
        apply_effects(
            &effects,
            Q,
            Some(&vmad),
            &world,
            &mut stages,
            &mut objectives,
            &mut deferred,
        )
    };
    assert!(advances.is_empty());

    // StartCombat applies immediately, before the guard scope drops.
    let player = world.resource::<PapyrusPlayerEntity>().0;
    let combat_state = world
        .get::<crate::AiCombatState>(attacker)
        .expect("StartCombat must arm AiCombatState on the resolved actor");
    assert_eq!(combat_state.target, player);
    assert_eq!(combat_state.attack_cooldown_remaining, 0.0);
    // #5046 — StartCombat takes ownership: with the ambient marker left in
    // place, `disengage_lost_contact` would read this same-target fight as
    // ambient and drop it after DISENGAGE_GRACE_SECS out of contact.
    assert!(
        world.get::<crate::AmbientEngagement>(attacker).is_none(),
        "StartCombat must clear the ambient marker so the scripted fight is \
         never dropped for lost contact"
    );

    // SetEnemy is deferred: not visible until the guard scope drops.
    assert!(world.resource::<crate::FactionRelations>().is_empty());
    deferred.apply(&world);
    let relations = world.resource::<crate::FactionRelations>();
    use byroredux_sdk::relationships::CombatReaction;
    assert_eq!(
        relations.reaction(stormcloaks, player_faction),
        Some(CombatReaction::Enemy)
    );
    assert_eq!(
        relations.reaction(player_faction, stormcloaks),
        Some(CombatReaction::Neutral)
    );
    drop(relations);
    drop(combat_state);

    // #4816 — `StopCombat` ends it again, on the same resolved actor.
    let stop = [Effect::StopCombat {
        actor: crate::translate::effects::ActorRef::Object(ObjectRef::Property(
            "Alias_TrophyRoomPrisoner01".into(),
        )),
    }];
    let mut deferred = DeferredFragmentEffects::new(&world);
    {
        let (mut stages, mut objectives) =
            world.resource_2_mut::<QuestStageState, QuestObjectiveState>();
        apply_effects(
            &stop,
            Q,
            Some(&vmad),
            &world,
            &mut stages,
            &mut objectives,
            &mut deferred,
        );
    }
    deferred.apply(&world);
    assert!(
        world.get::<crate::AiCombatState>(attacker).is_none(),
        "StopCombat must clear the actor's AiCombatState"
    );

    // #5017 — `SetUnconscious` knocks the same actor out mid-combat: the
    // state is set, restrained is cleared, and its combat ends. `false`
    // wakes it and leaves combat to re-derive.
    crate::update_actor_control(&world, attacker, |state| state.set_restrained(true));
    let actor = || {
        crate::translate::effects::ActorRef::Object(ObjectRef::Property(
            "Alias_TrophyRoomPrisoner01".into(),
        ))
    };
    let run = |effects: &[Effect]| {
        let mut deferred = DeferredFragmentEffects::new(&world);
        {
            let (mut stages, mut objectives) =
                world.resource_2_mut::<QuestStageState, QuestObjectiveState>();
            apply_effects(
                effects,
                Q,
                Some(&vmad),
                &world,
                &mut stages,
                &mut objectives,
                &mut deferred,
            );
        }
        deferred.apply(&world);
    };
    run(&[
        Effect::StartCombat {
            actor: actor(),
            target: crate::translate::effects::ActorRef::Player,
        },
        Effect::SetUnconscious {
            actor: actor(),
            unconscious: true,
        },
    ]);
    let state = *world.get::<crate::ActorControlState>(attacker).unwrap();
    assert!(state.unconscious && !state.restrained);
    assert!(
        world.get::<crate::AiCombatState>(attacker).is_none(),
        "an unconscious actor cannot fight"
    );
    run(&[Effect::SetUnconscious {
        actor: actor(),
        unconscious: false,
    }]);
    assert!(!crate::is_unconscious(&world, attacker));

    // M47.2 — `Kill` queues a death request for the engine's death path,
    // with the player as the killer.
    run(&[Effect::Kill {
        actor: actor(),
        killer: Some(crate::translate::effects::ActorRef::Player),
        silent: false,
    }]);
    let player = world.resource::<crate::papyrus_demo::PapyrusPlayerEntity>().0;
    assert_eq!(
        world.resource::<crate::story_manager::PendingScriptKills>().0,
        vec![(attacker, Some(player), false)]
    );
}

/// #5298 — the player-decline rule for the NPC-only arms is checked on the
/// *resolved* actor, not the lowered spelling. Skyrim's ubiquitous
/// `Actor Property PlayerRef Auto` lowers as `ActorRef::Object` (VMAD
/// `Object { form_id: 0x14, alias: -1 }`), which since #4694 resolves to
/// the player entity at runtime — so the syntactic `ActorRef::Player`
/// check in the lowering let these through. Pre-fix,
/// `PlayerRef.SetUnconscious(true)` cleared the player's `restrained`
/// flag mid-cinematic and `PlayerRef.StartCombat` armed the NPC combat
/// runtime on the player (#4323's impact).
#[test]
fn player_ref_property_receivers_decline_on_all_three_npc_only_arms() {
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    let world = fixture();
    let player = world.resource::<PapyrusPlayerEntity>().0;
    // The state a cinematic leaves the player in: restrained. A
    // PlayerRef-driven SetUnconscious used to clear exactly this.
    crate::update_actor_control(&world, player, |state| state.set_restrained(true));

    let vmad = ScriptInstanceData {
        scripts: vec![ScriptInstance {
            name: "QuestScript".into(),
            status: 0,
            properties: vec![ScriptProperty {
                name: "PlayerRef".into(),
                status: 1,
                value: PropertyValue::Object {
                    form_id: 0x14,
                    alias: -1,
                },
            }],
        }],
        ..Default::default()
    };
    let player_ref = || {
        crate::translate::effects::ActorRef::Object(ObjectRef::Property("PlayerRef".into()))
    };
    let effects = [
        Effect::SetUnconscious {
            actor: player_ref(),
            unconscious: true,
        },
        Effect::StartCombat {
            actor: player_ref(),
            target: crate::translate::effects::ActorRef::Player,
        },
        Effect::StopCombat {
            actor: player_ref(),
        },
    ];
    let mut deferred = DeferredFragmentEffects::new(&world);
    {
        let (mut stages, mut objectives) =
            world.resource_2_mut::<QuestStageState, QuestObjectiveState>();
        apply_effects(
            &effects,
            Q,
            Some(&vmad),
            &world,
            &mut stages,
            &mut objectives,
            &mut deferred,
        );
    }
    deferred.apply(&world);

    let state = *world.get::<crate::ActorControlState>(player).unwrap();
    assert!(
        !state.unconscious,
        "PlayerRef.SetUnconscious must not reach the player's ActorControlState"
    );
    assert!(
        state.restrained,
        "the mid-cinematic restraint must survive PlayerRef.SetUnconscious (#5298)"
    );
    assert!(
        world.get::<crate::AiCombatState>(player).is_none(),
        "PlayerRef.StartCombat must not steer the player's combat runtime"
    );
}

/// Regression for #2539: lifecycle metadata must come from a snapshot captured
/// before the paired quest-state guards, and alias invalidation must remain
/// queued until those guards drop.
#[test]
fn quest_definition_reads_and_alias_dirtying_bracket_quest_guards() {
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    let mut world = fixture();
    crate::install_start_game_quests(
        &mut world,
        [byroredux_plugin::esm::records::QustRecord {
            form_id: Q.0,
            start_up_stage: Some(5),
            shut_down_stage: Some(90),
            objectives: vec![
                byroredux_plugin::esm::records::QuestObjective {
                    index: 10,
                    ..Default::default()
                },
                byroredux_plugin::esm::records::QuestObjective {
                    index: 20,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
    );
    crate::refresh_scene_actor_bindings(&world);
    assert!(!world.resource::<crate::SceneActorBindings>().is_dirty());

    let vmad = ScriptInstanceData {
        scripts: vec![ScriptInstance {
            name: "QF_LifecycleLockRegression".into(),
            status: 0,
            properties: vec![ScriptProperty {
                name: "LifecycleQuest".into(),
                status: 1,
                value: PropertyValue::Object {
                    form_id: Q.0,
                    alias: -1,
                },
            }],
        }],
        ..Default::default()
    };
    let effects = [
        Effect::StartScene {
            scene: crate::translate::compose::ObjectRef::Property("LifecycleQuest".into()),
        },
        Effect::CompleteAllObjectives {
            quest: QuestRef::SelfRef,
        },
        Effect::StopScene {
            scene: crate::translate::compose::ObjectRef::Property("LifecycleQuest".into()),
        },
    ];

    let mut deferred = DeferredFragmentEffects::new(&world);
    *world.resource_mut::<crate::QuestDefinitionRegistry>() = Default::default();
    assert!(!world
        .resource::<crate::QuestDefinitionRegistry>()
        .contains(Q));

    let advances = {
        let (mut stages, mut objectives) =
            world.resource_2_mut::<QuestStageState, QuestObjectiveState>();
        apply_effects(
            &effects,
            Q,
            Some(&vmad),
            &world,
            &mut stages,
            &mut objectives,
            &mut deferred,
        )
    };

    assert_eq!(
        advances
            .iter()
            .map(|advance| advance.new_stage)
            .collect::<Vec<_>>(),
        [5, 90]
    );
    {
        let objectives = world.resource::<QuestObjectiveState>();
        assert!(objectives.get(Q, 10).completed);
        assert!(objectives.get(Q, 20).completed);
    }
    assert!(
        !world.resource::<crate::SceneActorBindings>().is_dirty(),
        "alias invalidation must remain queued until quest guards are released"
    );

    deferred.apply(&world);
    assert!(world.resource::<crate::SceneActorBindings>().is_dirty());
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

    // The empty-fragment tick must not consume the journal transition. Once
    // the table is populated, the same transition is still dispatched.
    world.resource_mut::<QuestStageFragments>().insert(
        Q,
        10,
        vec![Effect::SetObjectiveCompleted {
            quest: QuestRef::SelfRef,
            objective: 1,
            completed: true,
        }],
    );
    quest_fragment_dispatch_system(&world);
    assert!(world.resource::<QuestObjectiveState>().get(Q, 1).completed);
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
fn provider_aware_fragment_population_resumes_after_native_call() {
    use std::sync::{Arc, Mutex};

    use byroredux_papyrus::parse_script;
    use byroredux_sdk::script_function::ScriptValue;

    let (script, errors) = parse_script(
        "ScriptName QF_Test extends Quest\n\
         Function Fragment_0()\n\
         Utility.Wait(0.0)\n\
         Game.GetModCount()\n\
         Self.SetStage(20)\n\
         EndFunction\n",
    )
    .unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let providers = crate::PapyrusProviderCatalog::engine_compatibility();
    let principal =
        byroredux_sdk::identity::PrincipalId::new("legacy.scripts.quest-fragment").unwrap();
    let world = fixture();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let calls_for_callback = Arc::clone(&calls);
    let callback = Arc::new(
        move |principal: Option<&byroredux_sdk::identity::PrincipalId>,
              route: &str,
              arguments: &[ScriptValue]| {
            calls_for_callback.lock().unwrap().push((
                principal.map(ToString::to_string),
                route.to_owned(),
                arguments.to_vec(),
            ));
            Ok(ScriptValue::Integer(1))
        },
    ) as Arc<crate::PapyrusProviderCallback>;
    crate::set_papyrus_provider_runtime(&world, Arc::new(providers.clone()), Some(callback));
    {
        let mut fragments = world.resource_mut::<QuestStageFragments>();
        assert_eq!(
            populate_owned_quest_fragments_from_script_with_providers(
                &mut fragments,
                Q,
                &script,
                &[(10, "Fragment_0")],
                OwnedFragmentProviders::new(&providers, &principal),
            ),
            1
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);

    quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<FragmentExecutionQueue>().len(), 1);
    fragment_continuation_system(&world, 0.0);

    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &[(
            Some(principal.to_string()),
            byroredux_sdk::compatibility::PAPYRUS_GAME_GET_MOD_COUNT_ROUTE.to_owned(),
            Vec::new(),
        )]
    );
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
fn populate_from_script_appends_duplicate_stage_bindings_idempotently() {
    use byroredux_papyrus::parse_script;

    let (script, errs) = parse_script(
        "ScriptName QF_DuplicateStage_0 extends Quest\n\
         Function Fragment_0()\n Self.SetObjectiveDisplayed(10)\n EndFunction\n\
         Function Fragment_1()\n Self.SetObjectiveCompleted(20)\n EndFunction\n",
    )
    .expect("QF_ script parses");
    assert!(errs.is_empty(), "{errs:?}");
    let bindings = [(0u16, "Fragment_0"), (0u16, "Fragment_1")];

    let world = fixture();
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        assert_eq!(
            populate_quest_fragments_from_script(&mut frags, Q, &script, &bindings),
            2
        );
        assert_eq!(
            populate_quest_fragments_from_script(&mut frags, Q, &script, &bindings),
            2,
            "repopulation lowers both bindings again"
        );
        assert_eq!(
            frags.get(Q, 0).map(<[Effect]>::len),
            Some(2),
            "duplicate stage bindings append in VMAD order without duplicating on reload"
        );
    }

    emit_advance(&world, Q, 0);
    quest_fragment_dispatch_system(&world);
    let objectives = world.resource::<QuestObjectiveState>();
    assert!(objectives.get(Q, 10).displayed);
    assert!(objectives.get(Q, 20).completed);
}

#[test]
fn populate_from_script_skips_absent_and_declined_fragments() {
    use byroredux_papyrus::parse_script;

    let (script, errs) = parse_script(
        "ScriptName QF_X_0 extends Quest\n\
         Function Fragment_0()\n Self.SetStage(20)\n EndFunction\n\
         Function Fragment_1()\n Game.QuitToMainMenu()\n EndFunction\n",
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

/// Regression for #2657 (SCR-D5-NEW11-01), pinned end-to-end through the
/// REAL production entry point (`populate_quest_fragments_from_script`),
/// not just the lower-level `classify_effect`/`lower_fragment_with_
/// quest_properties` primitives `translate::effects`'s own test already
/// covers. A decompiled `.pex` auto-property read arrives as its backing
/// variable (`::MQ101_var`), a shape the `.psc` `Ident` regex can never
/// produce — `first_fn_body`-style `parse_script` fixtures elsewhere in
/// this file cannot express this AST, so it's hand-built here, mirroring
/// `translate::effects::tests::sp`.
///
/// Confirmed by direct code inspection that `quest_property_key`'s
/// `::`/`_var`-stripping normalization (added under #2653, `53f7de9d`)
/// already fixes the key-space mismatch #2657 reported — this test locks
/// that fix in against the exact real-input shape, through the exact
/// call path production uses, so a future refactor of either
/// `quest_property_names` or `receiver_object`/`explicit_quest_receiver`
/// can't silently reopen it.
#[test]
fn populate_from_script_resolves_decompiled_backing_variable_quest_property() {
    use byroredux_papyrus::ast::{Function, Identifier, Property};

    fn sp<T>(node: T) -> Spanned<T> {
        Spanned::new(node, byroredux_papyrus::span::Span::new(0, 0))
    }

    let script = Script {
        name: sp(Identifier("QF_TestQuest".into())),
        parent: Some(sp(Identifier("Quest".into()))),
        flags: byroredux_papyrus::ast::ScriptFlags::empty(),
        body: vec![
            sp(ScriptItem::Property(Box::new(Property {
                ty: sp(Type::Object(Identifier("Quest".into()))),
                name: sp(Identifier("MQ101".into())),
                flags: byroredux_papyrus::ast::PropertyFlags::AUTO,
                initial_value: None,
                getter: None,
                setter: None,
                doc_comment: None,
            }))),
            sp(ScriptItem::Function(Function {
                return_type: None,
                name: sp(Identifier("Fragment_99".into())),
                params: Vec::new(),
                flags: byroredux_papyrus::ast::FunctionFlags::empty(),
                // The decompiler's actual shape for `MQ101.Start()`:
                // `::MQ101_var.Start()`, not the `.psc`-expressible
                // `MQ101.Start()`.
                body: vec![sp(Stmt::ExprStmt(sp(byroredux_papyrus::ast::Expr::Call {
                    callee: Box::new(sp(byroredux_papyrus::ast::Expr::MemberAccess {
                        object: Box::new(sp(byroredux_papyrus::ast::Expr::Ident(Identifier(
                            "::MQ101_var".into(),
                        )))),
                        member: sp(Identifier("Start".into())),
                    })),
                    args: Vec::new(),
                })))],
                doc_comment: None,
            })),
        ],
    };

    let bindings = [(1u16, "Fragment_99")];
    let world = fixture();
    let inserted = {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        populate_quest_fragments_from_script(&mut frags, Q, &script, &bindings)
    };
    assert_eq!(
        inserted, 1,
        "MQ101.Start() (decompiled shape) must lower to StartQuest and \
         register, not decline the fragment (#2657)"
    );
    let frags = world.resource::<QuestStageFragments>();
    assert!(
        frags.get(Q, 1).is_some(),
        "the fragment must be registered for stage 1"
    );
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
                silent: false,
                container: crate::translate::effects::ActorRef::Object(crate::translate::compose::ObjectRef::Property("SomeContainer".into())),
                item: crate::translate::compose::ObjectRef::Property("SomeItem".into()),
                count: crate::translate::effects::ItemCount::Literal(3),
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
                silent: false,
                container: crate::translate::effects::ActorRef::Object(crate::translate::compose::ObjectRef::Property("SomeContainer".into())),
                item: crate::translate::compose::ObjectRef::Property("SomeItem".into()),
                count: crate::translate::effects::ItemCount::Literal(3),
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

/// Run one `MoveTo` fragment from the origin to a marker at (10, 20, 30)
/// rotated a quarter turn about Y, and return where the moved reference ends up.
fn run_move_to(offset: [f32; 3], match_rotation: bool) -> (byroredux_core::math::Vec3, byroredux_core::math::Quat) {
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
        GlobalTransform::new(Vec3::new(10.0, 20.0, 30.0), Quat::from_rotation_y(1.0), 1.0),
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
                offset,
                match_rotation,
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);
    quest_fragment_dispatch_system(&world);

    let transform = world.get::<Transform>(moved).expect("still present");
    (transform.translation, transform.rotation)
}

#[test]
fn dispatch_move_to_via_registered_vmad() {
    use byroredux_core::math::{Quat, Vec3};
    let (translation, rotation) = run_move_to([0.0; 3], true);
    assert_eq!(translation, Vec3::new(10.0, 20.0, 30.0));
    assert_eq!(rotation, Quat::from_rotation_y(1.0), "the default matches the marker's rotation");
}

/// CK *MoveTo*: the offset is in Gamebryo's Z-up world axes (engine Y-up
/// `(x, z, -y)`), and `abMatchRotation = False` leaves the rotation alone.
#[test]
fn move_to_applies_its_offset_and_rotation_flag() {
    use byroredux_core::math::{Quat, Vec3};
    let (translation, rotation) = run_move_to([1.0, 2.0, 3.0], false);
    assert_eq!(translation, Vec3::new(10.0 + 1.0, 20.0 + 3.0, 30.0 - 2.0));
    assert_eq!(rotation, Quat::IDENTITY);
}

/// M47.2 — an item count read from a global applies the global's value when
/// the fragment runs, truncated toward zero; a negative value skips the effect.
#[test]
fn item_count_from_a_global_reads_it_when_applied() {
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };
    use crate::translate::effects::ItemCount;
    const CHEST_FORM: u32 = 0x0000_6100;
    const GOLD_FORM: u32 = 0x0000_000F;
    const PRICE_FORM: u32 = 0x0000_6200;
    let mut world = fixture();
    let chest = spawn_with_form_id(&mut world, CHEST_FORM);
    let mut globals = crate::Globals::default();
    globals.set(PRICE_FORM, 7.9);
    world.insert_resource(globals);
    let object = |name: &str, form_id| ScriptProperty {
        name: name.into(),
        status: 1,
        value: PropertyValue::Object { form_id, alias: -1 },
    };
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
                        object("Chest", CHEST_FORM),
                        object("Gold001", GOLD_FORM),
                        object("Price", PRICE_FORM),
                    ],
                }],
            },
        );
        frags.insert(
            Q,
            10,
            vec![Effect::AddItem {
                silent: false,
                container: ActorRef::Object(crate::translate::compose::ObjectRef::Property(
                    "Chest".into(),
                )),
                item: crate::translate::compose::ObjectRef::Property("Gold001".into()),
                count: ItemCount::Global(crate::translate::compose::ObjectRef::Property(
                    "Price".into(),
                )),
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);
    quest_fragment_dispatch_system(&world);
    let inventory = world.get::<Inventory>(chest).expect("inventory created");
    assert_eq!(inventory.items[0].base_form_id, GOLD_FORM);
    assert_eq!(inventory.items[0].count, 7, "7.9 truncates toward zero");
}

/// M47.2 — `AddPerk` grants rank 1 (an owned perk keeps its rank) and
/// `RemovePerk` drops it, creating `Perks` on an actor that has none.
#[test]
fn perk_changes_apply_to_the_actors_perks() {
    use byroredux_core::character::Perks;
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };
    const ACTOR: u32 = 0x0000_6400;
    const PERK: u32 = 0x0000_6500;
    const RANKED: u32 = 0x0000_6501;
    let mut world = fixture();
    world.register::<Perks>();
    let actor = spawn_with_form_id(&mut world, ACTOR);
    let mut owned = Perks::default();
    owned.set_rank(RANKED, 3);
    world.insert(actor, owned);
    let object = |name: &str, form_id| ScriptProperty {
        name: name.into(),
        status: 1,
        value: PropertyValue::Object { form_id, alias: -1 },
    };
    let change = |perk: &str, add| Effect::ChangePerk {
        actor: crate::translate::effects::ActorRef::Object(ObjectRef::Property("Actor".into())),
        perk: ObjectRef::Property(perk.into()),
        add,
    };
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
                        object("Actor", ACTOR),
                        object("Perk", PERK),
                        object("Ranked", RANKED),
                    ],
                }],
            },
        );
        frags.insert(Q, 10, vec![change("Perk", true), change("Ranked", true)]);
        frags.insert(Q, 20, vec![change("Perk", false)]);
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);
    quest_fragment_dispatch_system(&world);
    {
        let perks = world.get::<Perks>(actor).unwrap();
        assert_eq!(perks.rank(PERK), 1);
        assert_eq!(perks.rank(RANKED), 3, "an owned perk keeps its rank");
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 20);
    emit_advance(&world, Q, 20);
    quest_fragment_dispatch_system(&world);
    assert_eq!(world.get::<Perks>(actor).unwrap().rank(PERK), 0);
}

/// M47.2 — `G = S + offset` reads the source global when it applies.
#[test]
fn relative_global_write_reads_its_source_when_applied() {
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };
    const READY: u32 = 0x0000_6300;
    const DAYS: u32 = 0x0000_0039;
    let mut world = fixture();
    let mut globals = crate::Globals::default();
    globals.set(DAYS, 12.5);
    globals.set(READY, 0.0);
    world.insert_resource(globals);
    let object = |name: &str, form_id| ScriptProperty {
        name: name.into(),
        status: 1,
        value: PropertyValue::Object { form_id, alias: -1 },
    };
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
                    properties: vec![object("Ready", READY), object("Days", DAYS)],
                }],
            },
        );
        frags.insert(
            Q,
            10,
            vec![
                Effect::SetGlobalValue {
                    global: ObjectRef::Property("Ready".into()),
                    value: crate::translate::effects::ValueExpr::Arith(
                        Box::new(crate::translate::effects::ValueExpr::Global(
                            ObjectRef::Property("Days".into()),
                        )),
                        crate::translate::effects::ArithOp::Add,
                        Box::new(crate::translate::effects::ValueExpr::Literal(1.0)),
                    ),
                },
                Effect::SetGlobalValue {
                    global: ObjectRef::Property("Ready".into()),
                    value: crate::translate::effects::ValueExpr::Arith(
                        Box::new(crate::translate::effects::ValueExpr::Global(
                            ObjectRef::Property("Ready".into()),
                        )),
                        crate::translate::effects::ArithOp::Add,
                        Box::new(crate::translate::effects::ValueExpr::Literal(0.5)),
                    ),
                },
            ],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);
    quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<crate::Globals>().get(READY), Some(14.0));
}

/// M47.2 — the player's `MoveTo` records the destination reference (not
/// loaded here: it may be in another cell) for the engine to drain.
#[test]
fn dispatch_player_move_to_requests_the_destination_reference() {
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };
    const DEST_FORM: u32 = 0x0000_7001;
    let world = fixture();
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
                    properties: vec![ScriptProperty {
                        name: "StartMarker".into(),
                        status: 1,
                        value: PropertyValue::Object {
                            form_id: DEST_FORM,
                            alias: -1,
                        },
                    }],
                }],
            },
        );
        frags.insert(
            Q,
            10,
            vec![Effect::MovePlayerTo {
                destination: crate::translate::compose::ObjectRef::Property("StartMarker".into()),
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);
    quest_fragment_dispatch_system(&world);

    assert_eq!(
        world.resource::<crate::PlayerMoveRequest>().0,
        Some(DEST_FORM)
    );
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

/// #3278 (SCR-D5-2026-08-24-01) regression — an alias-bound `Disable()`
/// receiver must resolve and dispatch, matching its sibling object-targeting
/// effects.
///
/// `prim_disable` lowers its receiver through the same `receiver_object` as
/// `AddItem`/`MoveTo`/`EquipItem`, so it can legitimately bind to a
/// quest-alias-filled `ObjectReference Property`. Dispatch, however, went
/// through the strict `resolve_property_form_id`, which only sees direct
/// VMAD FormID properties — so this exact shape silently declined while the
/// same alias-bound receiver worked for every sibling effect.
///
/// Also pins the round-trip the fix depends on: alias → entity → form ID,
/// keyed the way `ReferenceEnableState` stores it.
#[test]
fn dispatch_disable_resolves_an_alias_bound_receiver() {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    const MARKER_FORM: u32 = 0x0004_B1CE;
    const MARKER_ALIAS: i16 = 3;

    let mut world = fixture();
    let mut pool = FormIdPool::new();
    let marker_id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(MARKER_FORM),
    });
    world.insert_resource(pool);

    // The alias-filled scene marker: a live entity carrying its form ID,
    // reachable only through the quest's alias bindings.
    let marker = world.spawn();
    world.insert(marker, FormIdComponent(marker_id));
    world
        .resource_mut::<crate::SceneActorBindings>()
        .bind(Q, i32::from(MARKER_ALIAS), marker);

    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert_vmad(
            Q,
            ScriptInstanceData {
                scripts: vec![ScriptInstance {
                    name: "QF_AliasDisable".into(),
                    status: 0,
                    properties: vec![ScriptProperty {
                        name: "Marker".into(),
                        status: 1,
                        // alias >= 0: filled by the quest, no direct FormID.
                        // This is what `resolve_property_form_id` cannot see.
                        value: PropertyValue::Object {
                            form_id: Q.0,
                            alias: MARKER_ALIAS,
                        },
                    }],
                }],
                ..Default::default()
            },
        );
        frags.insert(
            Q,
            40,
            vec![Effect::Disable {
                object: ObjectRef::Property("Marker".into()),
                fade_out: false,
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 40);
    emit_advance(&world, Q, 40);
    quest_fragment_dispatch_system(&world);

    assert!(
        !world
            .resource::<crate::ReferenceEnableState>()
            .is_enabled(MARKER_FORM),
        "#3278: an alias-bound Disable() must record the disable; pre-fix the \
         strict resolver returned None and the effect silently declined"
    );
}

#[test]
fn dispatch_disable_then_stage_cascade_and_package_evaluation() {
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    const MARKER_FORM: u32 = 0x0005_AA55;
    const HORSE_1_ALIAS: i16 = 1;
    const HORSE_2_ALIAS: i16 = 2;
    let mut world = fixture();
    let horse_1 = world.spawn();
    let horse_2 = world.spawn();
    {
        let mut bindings = world.resource_mut::<crate::SceneActorBindings>();
        bindings.bind(Q, i32::from(HORSE_1_ALIAS), horse_1);
        bindings.bind(Q, i32::from(HORSE_2_ALIAS), horse_2);
    }
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert_vmad(
            Q,
            ScriptInstanceData {
                scripts: vec![ScriptInstance {
                    name: "QF_DisableCascade".into(),
                    status: 0,
                    properties: vec![
                        ScriptProperty {
                            name: "Marker".into(),
                            status: 1,
                            value: PropertyValue::Object {
                                form_id: MARKER_FORM,
                                alias: -1,
                            },
                        },
                        ScriptProperty {
                            name: "Horse1".into(),
                            status: 1,
                            value: PropertyValue::Object {
                                form_id: Q.0,
                                alias: HORSE_1_ALIAS,
                            },
                        },
                        ScriptProperty {
                            name: "Horse2".into(),
                            status: 1,
                            value: PropertyValue::Object {
                                form_id: Q.0,
                                alias: HORSE_2_ALIAS,
                            },
                        },
                    ],
                }],
                ..Default::default()
            },
        );
        frags.insert(
            Q,
            30,
            vec![
                Effect::Disable {
                    object: ObjectRef::Property("Marker".into()),
                    fade_out: false,
                },
                Effect::SetStage {
                    quest: QuestRef::SelfRef,
                    stage: 27,
                },
                Effect::EvaluatePackage {
                    actor: ActorRef::Object(ObjectRef::Property("Horse1".into())),
                },
                Effect::EvaluatePackage {
                    actor: ActorRef::Object(ObjectRef::Property("Horse2".into())),
                },
            ],
        );
        frags.insert(
            Q,
            27,
            vec![Effect::SetStage {
                quest: QuestRef::SelfRef,
                stage: 26,
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 30);
    emit_advance(&world, Q, 30);

    quest_fragment_dispatch_system(&world);

    let stages = world.resource::<QuestStageState>();
    assert!(stages.get_stage_done(Q, 27));
    assert!(stages.get_stage_done(Q, 26));
    drop(stages);
    assert!(!world
        .resource::<ReferenceEnableState>()
        .is_enabled(MARKER_FORM));
    assert!(world.has::<crate::EvaluatePackageRequest>(horse_1));
    assert!(world.has::<crate::EvaluatePackageRequest>(horse_2));
}

/// #3489 regression — `Effect::Enable` must reach `ReferenceEnableState`
/// and clear the disabled entry, the direction `set_enabled`'s
/// `enabled == true` branch had no production caller to exercise before
/// this fix. Mirrors `dispatch_disable_then_stage_cascade_and_package_
/// evaluation`'s direct-FormID shape.
#[test]
fn dispatch_enable_clears_a_previously_disabled_reference() {
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    const MARKER_FORM: u32 = 0x0005_BB66;
    let world = fixture();
    world
        .resource_mut::<ReferenceEnableState>()
        .set_enabled(MARKER_FORM, false);
    assert!(
        !world
            .resource::<ReferenceEnableState>()
            .is_enabled(MARKER_FORM),
        "test setup: the marker must start disabled"
    );
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert_vmad(
            Q,
            ScriptInstanceData {
                scripts: vec![ScriptInstance {
                    name: "QF_Enable".into(),
                    status: 0,
                    properties: vec![ScriptProperty {
                        name: "Marker".into(),
                        status: 1,
                        value: PropertyValue::Object {
                            form_id: MARKER_FORM,
                            alias: -1,
                        },
                    }],
                }],
                ..Default::default()
            },
        );
        frags.insert(
            Q,
            50,
            vec![Effect::Enable {
                object: ObjectRef::Property("Marker".into()),
                fade_in: false,
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 50);
    emit_advance(&world, Q, 50);
    quest_fragment_dispatch_system(&world);

    assert!(
        world
            .resource::<ReferenceEnableState>()
            .is_enabled(MARKER_FORM),
        "#3489: Enable() must clear the disabled entry, not leave it \
         permanently disabled"
    );
}

/// #4136 — a scripted lock change must also reach the persistent
/// `ReferenceLockState` ledger, not just the live component.
///
/// The component alone loses the change twice: across a save/load (it is
/// not a registered column) and across an ordinary in-session cell
/// revisit (cell unload despawns the entity, and the loader re-stamps the
/// plugin's authored XLOC onto a fresh placement root). The ledger is
/// FormID-keyed so it outlives both.
#[test]
fn dispatch_set_locked_records_the_change_in_the_persistent_ledger() {
    use crate::{LockOverride, ReferenceLockState};
    use byroredux_core::ecs::components::Locked;
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    const DOOR_FORM: u32 = 0x0009_0C22;
    let mut world = fixture();
    let door = spawn_with_form_id(&mut world, DOOR_FORM);
    world.register::<Locked>();
    world.insert(
        door,
        Locked {
            lock_level: 75,
            key_form_id: Some(0x1234),
        },
    );

    let vmad = ScriptInstanceData {
        scripts: vec![ScriptInstance {
            name: "QF_LOCK".into(),
            status: 0,
            properties: vec![ScriptProperty {
                name: "MyDoor".into(),
                status: 1,
                value: PropertyValue::Object {
                    form_id: DOOR_FORM,
                    alias: -1,
                },
            }],
        }],
        ..Default::default()
    };
    let door_ref = || crate::translate::compose::ObjectRef::Property("::MyDoor_var".into());

    assert_eq!(
        world
            .resource::<ReferenceLockState>()
            .override_for(DOOR_FORM),
        None,
        "nothing scripted yet — the authored XLOC must still be the only source"
    );

    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert_vmad(Q, vmad.clone());
        frags.insert(
            Q,
            10,
            vec![Effect::SetLocked {
                target: door_ref(),
                locked: false,
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);
    quest_fragment_dispatch_system(&world);

    assert!(
        world.get::<Locked>(door).is_none(),
        "the live unlock still happens"
    );
    assert_eq!(
        world
            .resource::<ReferenceLockState>()
            .override_for(DOOR_FORM),
        Some(LockOverride::Unlocked),
        "the unlock must be recorded so a cell reload does not re-stamp the \
         authored lock over it (#4136)"
    );

    // Re-locking records the resulting state, read back from the component
    // rather than re-derived — the re-lock branch keeps whatever the entity
    // already carried, so only the component knows the final level.
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert(
            Q,
            20,
            vec![Effect::SetLocked {
                target: door_ref(),
                locked: true,
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 20);
    emit_advance(&world, Q, 20);
    quest_fragment_dispatch_system(&world);

    assert_eq!(
        world
            .resource::<ReferenceLockState>()
            .override_for(DOOR_FORM),
        Some(LockOverride::Locked {
            lock_level: 0,
            key_form_id: None
        }),
        "a script-created lock has no authored XLOC to recover a difficulty \
         from, and the ledger must match what the component actually holds"
    );

    // And SetLockLevel moves the ledger's difficulty with the component's.
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert(
            Q,
            30,
            vec![Effect::SetLockLevel {
                target: door_ref(),
                level: 90,
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 30);
    emit_advance(&world, Q, 30);
    quest_fragment_dispatch_system(&world);

    assert_eq!(world.get::<Locked>(door).unwrap().lock_level, 90);
    assert_eq!(
        world
            .resource::<ReferenceLockState>()
            .override_for(DOOR_FORM),
        Some(LockOverride::Locked {
            lock_level: 90,
            key_form_id: None
        })
    );
}

/// #4329 — a first `SetLockLevel` on a keyed door must record the whole live
/// outcome, key included. Queuing only the level let the ledger invent
/// `key_form_id: None`, and the ledger outranks the authored `XLOC` on the
/// next load, so the door stopped accepting its own key after a revisit.
#[test]
fn dispatch_set_lock_level_on_an_untouched_keyed_door_keeps_the_key_in_the_ledger() {
    use crate::{LockOverride, ReferenceLockState};
    use byroredux_core::ecs::components::Locked;
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    const DOOR_FORM: u32 = 0x0009_0C23;
    let mut world = fixture();
    let door = spawn_with_form_id(&mut world, DOOR_FORM);
    world.register::<Locked>();
    world.insert(
        door,
        Locked {
            lock_level: 75,
            key_form_id: Some(0x1234),
        },
    );
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert_vmad(
            Q,
            ScriptInstanceData {
                scripts: vec![ScriptInstance {
                    name: "QF_LOCK".into(),
                    status: 0,
                    properties: vec![ScriptProperty {
                        name: "MyDoor".into(),
                        status: 1,
                        value: PropertyValue::Object {
                            form_id: DOOR_FORM,
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
            vec![Effect::SetLockLevel {
                target: crate::translate::compose::ObjectRef::Property("::MyDoor_var".into()),
                level: 90,
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);
    quest_fragment_dispatch_system(&world);

    let live = *world.get::<Locked>(door).unwrap();
    assert_eq!((live.lock_level, live.key_form_id), (90, Some(0x1234)));
    assert_eq!(
        world
            .resource::<ReferenceLockState>()
            .override_for(DOOR_FORM),
        Some(LockOverride::Locked {
            lock_level: 90,
            key_form_id: Some(0x1234)
        }),
        "the ledger must match the live component, key included (#4329)"
    );
}

/// #4330 — unlocking a direct reference whose cell is not loaded still
/// reaches the ledger, the `Disable` posture (#3278), so the door is unlocked
/// when its cell next loads. A re-lock of an unloaded reference records
/// nothing: with no live component there is no authored level or key to keep.
#[test]
fn dispatch_set_locked_on_an_unloaded_reference_records_only_an_unlock() {
    use crate::{LockOverride, ReferenceLockState};
    use byroredux_core::ecs::components::Locked;
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    const FAR_DOOR: u32 = 0x0009_0C24;
    const OTHER_DOOR: u32 = 0x0009_0C25;
    let mut world = fixture();
    world.register::<Locked>();
    // No entity carries either FormID: both references' cells are unloaded.
    let property = |name: &str, form_id: u32| ScriptProperty {
        name: name.into(),
        status: 1,
        value: PropertyValue::Object { form_id, alias: -1 },
    };
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert_vmad(
            Q,
            ScriptInstanceData {
                scripts: vec![ScriptInstance {
                    name: "QF_LOCK".into(),
                    status: 0,
                    properties: vec![
                        property("FarDoor", FAR_DOOR),
                        property("OtherDoor", OTHER_DOOR),
                    ],
                }],
                ..Default::default()
            },
        );
        frags.insert(
            Q,
            10,
            vec![
                Effect::SetLocked {
                    target: crate::translate::compose::ObjectRef::Property("::FarDoor_var".into()),
                    locked: false,
                },
                Effect::SetLocked {
                    target: crate::translate::compose::ObjectRef::Property(
                        "::OtherDoor_var".into(),
                    ),
                    locked: true,
                },
            ],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);
    quest_fragment_dispatch_system(&world);

    let ledger = world.resource::<ReferenceLockState>();
    assert_eq!(
        ledger.override_for(FAR_DOOR),
        Some(LockOverride::Unlocked),
        "an unlock of an unloaded reference must still be recorded (#4330)"
    );
    assert_eq!(
        ledger.override_for(OTHER_DOOR),
        None,
        "a re-lock with no live component has no authored lock to record"
    );
}

/// #3159 — the removal half. `Locked` had one insert (the cell loader's
/// XLOC stamp) and one read (the interaction gate) and nothing anywhere
/// that cleared it, so an authored lock was a one-way door for the whole
/// session and every quest depending on a scripted unlock was unfinishable.
#[test]
fn dispatch_set_locked_clears_and_restores_an_authored_lock() {
    use byroredux_core::ecs::components::Locked;
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    const DOOR_FORM: u32 = 0x0009_0B11;
    let mut world = fixture();
    let door = spawn_with_form_id(&mut world, DOOR_FORM);
    world.register::<Locked>();
    world.insert(
        door,
        Locked {
            lock_level: 75,
            key_form_id: Some(0x1234),
        },
    );

    let vmad = ScriptInstanceData {
        scripts: vec![ScriptInstance {
            name: "QF_LOCK".into(),
            status: 0,
            properties: vec![ScriptProperty {
                name: "MyDoor".into(),
                status: 1,
                value: PropertyValue::Object {
                    form_id: DOOR_FORM,
                    alias: -1,
                },
            }],
        }],
        ..Default::default()
    };
    let door_ref = || crate::translate::compose::ObjectRef::Property("::MyDoor_var".into());

    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert_vmad(Q, vmad.clone());
        frags.insert(
            Q,
            10,
            vec![Effect::SetLocked {
                target: door_ref(),
                locked: false,
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);
    quest_fragment_dispatch_system(&world);

    assert!(
        world.get::<Locked>(door).is_none(),
        "Lock(false) must remove the component — its presence IS the locked \
         state, and the interaction gate reads exactly that"
    );

    // ...and the inverse direction works, so this is a toggle rather than a
    // one-way door in the other direction.
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert(
            Q,
            20,
            vec![Effect::SetLocked {
                target: door_ref(),
                locked: true,
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 20);
    emit_advance(&world, Q, 20);
    quest_fragment_dispatch_system(&world);

    let relocked = world.get::<Locked>(door).expect("Lock(true) must re-lock");
    assert_eq!(
        relocked.lock_level, 0,
        "a script-created lock has no authored XLOC to recover a difficulty \
         from, so it records the least-restrictive shape rather than \
         inventing one"
    );
    assert_eq!(relocked.key_form_id, None);
}

/// #3159 — `SetLockLevel` sets difficulty and must never change lock state,
/// in either direction. Conflating it with `Lock` would make a fragment
/// that merely re-tunes a lock silently lock or unlock a door.
#[test]
fn dispatch_set_lock_level_changes_difficulty_without_touching_lock_state() {
    use byroredux_core::ecs::components::Locked;
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    const DOOR_FORM: u32 = 0x0009_0B12;
    const OPEN_FORM: u32 = 0x0009_0B13;
    let mut world = fixture();
    let locked_door = spawn_with_form_id(&mut world, DOOR_FORM);
    let unlocked_door = spawn_with_form_id(&mut world, OPEN_FORM);
    world.register::<Locked>();
    world.insert(
        locked_door,
        Locked {
            lock_level: 25,
            key_form_id: Some(0xABCD),
        },
    );

    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert_vmad(
            Q,
            ScriptInstanceData {
                scripts: vec![ScriptInstance {
                    name: "QF_LOCK".into(),
                    status: 0,
                    properties: vec![
                        ScriptProperty {
                            name: "MyDoor".into(),
                            status: 1,
                            value: PropertyValue::Object {
                                form_id: DOOR_FORM,
                                alias: -1,
                            },
                        },
                        ScriptProperty {
                            name: "OpenDoor".into(),
                            status: 1,
                            value: PropertyValue::Object {
                                form_id: OPEN_FORM,
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
                Effect::SetLockLevel {
                    target: crate::translate::compose::ObjectRef::Property("::MyDoor_var".into()),
                    level: 100,
                },
                Effect::SetLockLevel {
                    target: crate::translate::compose::ObjectRef::Property("::OpenDoor_var".into()),
                    level: 100,
                },
            ],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);
    quest_fragment_dispatch_system(&world);

    let still_locked = world
        .get::<Locked>(locked_door)
        .expect("SetLockLevel must not unlock");
    assert_eq!(still_locked.lock_level, 100, "difficulty must be updated");
    assert_eq!(
        still_locked.key_form_id,
        Some(0xABCD),
        "the authored key must survive a difficulty change"
    );

    assert!(
        world.get::<Locked>(unlocked_door).is_none(),
        "SetLockLevel on an unlocked object must stay a no-op — the absence \
         of the component IS the unlocked state, so creating one here would \
         lock a door the script never asked to lock"
    );
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
                    native_only: false,
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

    // #2654 — the activation is queued, not inserted: the real schedule
    // runs most `ActivateEvent` consumers *before* fragment
    // dispatch, so a marker inserted here would be drained at Stage::Late
    // having reached none of them. It must not be live yet...
    assert!(
        world.get::<crate::ActivateEvent>(lever).is_none(),
        "fragment activations are deferred, not inserted during dispatch"
    );
    assert_eq!(
        world.resource::<crate::PendingFragmentActivations>().len(),
        1
    );

    // ...and must become a real marker when the flush system runs at the
    // head of the next frame, ahead of every consumer.
    crate::fragment_activation_flush_system(&world, 0.016);
    assert_eq!(
        world.get::<crate::ActivateEvent>(lever).unwrap().activator,
        soldier
    );
    assert!(
        world
            .resource::<crate::PendingFragmentActivations>()
            .is_empty(),
        "the queue drains exactly once"
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
    let player = world.resource::<PapyrusPlayerEntity>().0;
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
                        ..crate::PlayerControlSelection::DISABLE_PLAYER_CONTROLS_DEFAULT
                    },
                },
                Effect::SetPlayerAiDriven { ai_driven: true },
                Effect::SetHudCartMode { cart_mode: true },
                Effect::EvaluatePackage {
                    actor: ActorRef::Object(crate::translate::compose::ObjectRef::Property(
                        "::Alias_Hadvar_var".into(),
                    )),
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
    let player = world.resource::<PapyrusPlayerEntity>().0;
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
    let player = world.resource::<PapyrusPlayerEntity>().0;
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

/// Regression: #5681 — the continuation queue is saved with the world, so
/// it is bounded: entries past `MAX_FRAGMENT_CONTINUATIONS` drop
/// newest-first (the same overflow policy as the provider continuation
/// queue), keeping the oldest tails. A mod that re-fires a fragment per
/// player action cannot grow the persisted queue without limit.
#[test]
fn continuation_queue_drops_newest_past_the_cap() {
    let world = fixture();
    let overflow = 8;
    {
        let mut queue = world.resource_mut::<FragmentExecutionQueue>();
        for i in 0..MAX_FRAGMENT_CONTINUATIONS + overflow {
            // `remaining_seconds` doubles as the entry's index, so the
            // survivors' order is observable. dt 0 below readies nothing.
            queue.pending.push(PendingFragmentExecution {
                context: Q,
                vmad: None,
                effects: Vec::new(),
                remaining_seconds: 100.0 + i as f32,
                resume_when: FragmentResumeCondition::DelayElapsed,
            });
        }
    }
    fragment_continuation_system(&world, 0.0);
    let queue = world.resource::<FragmentExecutionQueue>();
    assert_eq!(
        queue.len(),
        MAX_FRAGMENT_CONTINUATIONS,
        "the queue must be capped, not grown by every push"
    );
    assert_eq!(
        queue.pending.first().unwrap().remaining_seconds,
        100.0,
        "overflow drops the newest tails, so the oldest entry survives"
    );
    assert_eq!(
        queue.pending.last().unwrap().remaining_seconds,
        100.0 + MAX_FRAGMENT_CONTINUATIONS as f32 - 1.0
    );
}

/// Regression: #5984 — the #5681 cap used to apply only when timed entries
/// were re-partitioned. Zero-delay entries (budget deferrals, custom-event
/// deliveries, `Interpret` yields) never counted, so a self-re-sending
/// script grew the saved queue without bound. The cap now lives on the push.
#[test]
fn zero_delay_deferrals_are_capped_at_the_push() {
    let world = fixture();
    for _ in 0..MAX_FRAGMENT_CONTINUATIONS + 50 {
        // Ok(()) either way: a full queue sheds the newest tail rather than
        // handing it back to run unbudgeted.
        assert!(super::budget::defer(&world, Q, None, Vec::new()).is_ok());
    }
    assert_eq!(world.resource::<FragmentExecutionQueue>().len(), MAX_FRAGMENT_CONTINUATIONS);

    // The ready tail the exhausted-budget path re-queues is capped too.
    let mut queue = world.resource_mut::<FragmentExecutionQueue>();
    queue.extend_capped((0..10).map(|_| PendingFragmentExecution {
        context: Q,
        vmad: None,
        effects: Vec::new(),
        remaining_seconds: 0.0,
        resume_when: FragmentResumeCondition::DelayElapsed,
    }));
    assert_eq!(queue.len(), MAX_FRAGMENT_CONTINUATIONS);
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
    assert_eq!(
        world.get::<crate::EquipmentEventBatch>(rider).unwrap().0,
        vec![crate::EquipmentChange {
            item_form_id: GAG,
            equipped: true,
        }]
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

#[test]
fn bootstrap_batch_larger_than_cascade_cap_dispatches_every_initial_event() {
    let world = fixture();
    let quests = (0..=MAX_CASCADE)
        .map(|index| QuestFormId(0x0010_0000 + index as u32))
        .collect::<Vec<_>>();
    {
        let mut fragments = world.resource_mut::<QuestStageFragments>();
        for (index, quest) in quests.iter().copied().enumerate() {
            fragments.insert(
                quest,
                0,
                vec![Effect::SetObjectiveCompleted {
                    quest: QuestRef::SelfRef,
                    objective: index as i32,
                    completed: true,
                }],
            );
        }
    }
    let advances = quests
        .iter()
        .copied()
        .map(|quest| (quest, 0))
        .collect::<Vec<_>>();
    emit_advances(&world, &advances);

    quest_fragment_dispatch_system(&world);

    let objectives = world.resource::<QuestObjectiveState>();
    for (index, quest) in quests.iter().copied().enumerate() {
        assert!(
            objectives.get(quest, index as i32).completed,
            "initial event {index} was truncated by the cascade guard"
        );
    }
}

#[test]
fn scene_phase_fragment_advances_its_owning_quest() {
    use std::sync::{Arc, Mutex};

    use byroredux_papyrus::parse_script;
    use byroredux_plugin::esm::records::script_instance::SceneFragmentEvent;
    use byroredux_sdk::script_function::ScriptValue;

    let (script, errors) = parse_script(
        "ScriptName SF_TestScene_00000100 extends Scene\n\
         Function Fragment_0()\n Game.GetModCount()\n Self.GetOwningQuest().SetStage(14)\n EndFunction\n",
    )
    .expect("scene fragment script parses");
    assert!(errors.is_empty(), "{errors:?}");

    let mut world = fixture();
    let providers = crate::PapyrusProviderCatalog::engine_compatibility();
    let principal =
        byroredux_sdk::identity::PrincipalId::new("legacy.scripts.scene-fragment").unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&calls);
    let callback = Arc::new(
        move |principal: Option<&byroredux_sdk::identity::PrincipalId>,
              _route: &str,
              _arguments: &[ScriptValue]| {
            observed
                .lock()
                .unwrap()
                .push(principal.map(ToString::to_string));
            Ok(ScriptValue::Integer(1))
        },
    ) as Arc<crate::PapyrusProviderCallback>;
    crate::set_papyrus_provider_runtime(&world, Arc::new(providers.clone()), Some(callback));
    world.resource_mut::<QuestStageState>().start_quest(Q, None);
    let event = SceneFragmentEvent::PhaseCompletion { phase_index: 5 };
    let inserted = {
        let mut fragments = world.resource_mut::<SceneFragments>();
        populate_owned_scene_fragments_from_script_with_providers(
            &mut fragments,
            0x100,
            Q,
            None,
            &script,
            &[(event, "Fragment_0")],
            OwnedFragmentProviders::new(&providers, &principal),
        )
    };
    assert_eq!(inserted, 1);

    let scene_entity = world.spawn();
    world.insert(
        scene_entity,
        crate::scene::SceneFragmentInvocationBatch(vec![crate::scene::SceneFragmentInvocation {
            scene_form_id: 0x100,
            event,
            script_name: "SF_TestScene_00000100".to_owned(),
            fragment_name: "Fragment_0".to_owned(),
        }]),
    );
    scene_fragment_dispatch_system(&world, 0.0);

    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 14);
    assert_eq!(*calls.lock().unwrap(), [Some(principal.to_string())]);
    let player = world.resource::<PapyrusPlayerEntity>().0;
    let query = world.query::<QuestStageAdvancedBatch>().unwrap();
    let batch = query
        .get(player)
        .expect("scene SetStage compatibility batch");
    assert_eq!(batch.0.len(), 1);
    assert_eq!(batch.0[0].quest, Q);
    assert_eq!(batch.0[0].new_stage, 14);
}

#[test]
fn scene_fragments_flush_provider_barriers_before_the_next_invocation() {
    use std::sync::Arc;

    use byroredux_plugin::esm::records::script_instance::SceneFragmentEvent;

    use crate::translate::effects::FragmentProviderCall;
    use byroredux_sdk::script_function::ScriptValue;

    let mut world = fixture();
    let callback = Arc::new(
        |_principal: Option<&byroredux_sdk::identity::PrincipalId>,
         _route: &str,
         _arguments: &[ScriptValue]| Ok(ScriptValue::Integer(1)),
    ) as Arc<crate::PapyrusProviderCallback>;
    crate::set_papyrus_provider_runtime(
        &world,
        Arc::new(crate::PapyrusProviderCatalog::default()),
        Some(callback),
    );
    let event = SceneFragmentEvent::PhaseCompletion { phase_index: 5 };
    {
        let mut fragments = world.resource_mut::<SceneFragments>();
        fragments.insert(
            0x100,
            event,
            Q,
            None,
            vec![
                Effect::ProviderCall(FragmentProviderCall {
                    route: "ext.example.first-scene".to_owned(),
                    arguments: Vec::new(),
                    principal: None,
                }),
                Effect::SetStage {
                    quest: QuestRef::SelfRef,
                    stage: 10,
                },
            ],
        );
        fragments.insert(
            0x101,
            event,
            Q,
            None,
            vec![Effect::SetStage {
                quest: QuestRef::SelfRef,
                stage: 20,
            }],
        );
    }
    let scene_entity = world.spawn();
    world.insert(
        scene_entity,
        crate::scene::SceneFragmentInvocationBatch(vec![
            crate::scene::SceneFragmentInvocation {
                scene_form_id: 0x100,
                event,
                script_name: "SF_First".to_owned(),
                fragment_name: "Fragment_0".to_owned(),
            },
            crate::scene::SceneFragmentInvocation {
                scene_form_id: 0x101,
                event,
                script_name: "SF_Second".to_owned(),
                fragment_name: "Fragment_0".to_owned(),
            },
        ]),
    );

    scene_fragment_dispatch_system(&world, 0.0);

    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
    let player = world.resource::<PapyrusPlayerEntity>().0;
    let query = world.query::<QuestStageAdvancedBatch>().unwrap();
    let batch = query
        .get(player)
        .expect("scene SetStage compatibility batch");
    assert_eq!(
        batch
            .0
            .iter()
            .map(|advance| advance.new_stage)
            .collect::<Vec<_>>(),
        vec![10, 20]
    );
}

/// Offset of (and name of) the next top-level `fn` item at or after `from`.
///
/// #3854 — before the split, every item in `fragment.rs` was module-private
/// and these scans could look for a bare `"\nfn "`. Items now reached across
/// a submodule boundary carry a `pub(crate)` visibility prefix, so matching
/// the bare form would silently find the *wrong* item — or none — and these
/// tests would stop pinning what they were written to pin. Accepting the
/// prefix keeps them exact; the returned offset is of the leading newline, so
/// callers can still slice the preceding doc block.
fn next_top_level_fn(src: &str, from: usize) -> Option<(usize, &str)> {
    let tail = &src[from..];
    let mut at = 0usize;
    while let Some(nl) = tail[at..].find('\n') {
        let line_start = at + nl + 1;
        let line = &tail[line_start..];
        for prefix in ["fn ", "pub fn ", "pub(crate) fn ", "pub(super) fn ", "pub(in crate::fragment) fn "] {
            if let Some(rest) = line.strip_prefix(prefix) {
                let name = rest.split('(').next().unwrap_or("").trim();
                return Some((from + at + nl, name));
            }
        }
        at = line_start;
    }
    None
}

/// `apply_effect`'s own body concatenated with every
/// `apply_<family>_effect` helper body it delegates to (#4340).
///
/// The acquisitions the #3949 scan inventories used to sit directly in the
/// 680-line match; they now sit in the per-family helpers. Scanning only
/// `apply_effect` would find almost nothing and report green, so the scan
/// follows the delegation instead — a helper is not a place an unlisted
/// lock gets to hide.
fn apply_effect_family_bodies(src: &str) -> String {
    let mut bodies = String::new();
    let mut at = 0usize;
    let mut found = 0usize;
    while let Some((nl, name)) = next_top_level_fn(src, at) {
        at = nl + 1;
        if name != "apply_effect" && !(name.starts_with("apply_") && name.ends_with("_effect")) {
            continue;
        }
        let end = src[nl..]
            .find("\n}\n")
            .map(|offset| nl + offset)
            .unwrap_or(src.len());
        bodies.push_str(&src[nl..end]);
        bodies.push('\n');
        found += 1;
    }
    assert!(
        found > 1,
        "found only {found} apply_*_effect bodies — the delegation scan broke, \
         not the doc"
    );
    bodies
}

/// The bodies of the `crate::magic` functions the `AddSpell` /
/// `RemoveSpell` and racial-pair arms of `apply_effect` call out to
/// (#5071).
///
/// Those arms apply inline under the quest guards (unlike the deferred
/// `SetEnemy`), but their acquisitions sit in a different module — the
/// fragment-body scan above is structurally blind to them, which is how
/// `SpellList` / `SpellCatalog` / `ActorValues` / `RaceSpells` aged out of
/// the inventory. Every function named here must be found; a rename fails
/// loud instead of silently scanning green.
fn magic_callout_bodies(src: &str) -> String {
    const CALLOUTS: &[&str] = &[
        "add_spell",
        "remove_spell",
        "add_race_spells",
        "remove_race_spells",
        "apply_constant_modifiers",
    ];
    let mut bodies = String::new();
    for wanted in CALLOUTS {
        let mut at = 0usize;
        let mut found = false;
        while let Some((nl, name)) = next_top_level_fn(src, at) {
            at = nl + 1;
            if name != *wanted {
                continue;
            }
            let end = src[nl..]
                .find("\n}\n")
                .map(|offset| nl + offset)
                .unwrap_or(src.len());
            bodies.push_str(&src[nl..end]);
            bodies.push('\n');
            found = true;
            break;
        }
        assert!(
            found,
            "`{wanted}` not found in magic.rs — the #5071 call-out scan broke, \
             not the doc"
        );
    }
    bodies
}

/// The bodies of the helpers `apply_effects` / the `apply_*_effect` family call
/// that sit outside the family and outside `magic.rs` (#5986): the continuation
/// queue writes, the inventory transfers, the guard / value evaluators, the
/// actor resolvers and the equipment-event emitters. They run under the same
/// paired quest guards, so a lock one of them takes is a lock taken under it;
/// the family-only scan was blind to them. Every name must be found, so a rename
/// fails loud instead of scanning green.
fn nested_helper_bodies(sources: &str, equipment_rs: &str) -> String {
    const IN_SOURCES: &[&str] = &[
        "apply_effects",
        "resolve_actor",
        "update_actor_cinematic_state",
        "evaluate_guard",
        "evaluate_value",
        "defer",
        "remove_items",
        "add_items",
    ];
    const IN_EQUIPMENT: &[&str] = &["emit_equipment_changes", "emit_item_transfers"];
    let mut bodies = String::new();
    for (src, wanted, origin) in IN_SOURCES
        .iter()
        .map(|name| (sources, *name, "fragment::SOURCES"))
        .chain(IN_EQUIPMENT.iter().map(|name| (equipment_rs, *name, "equipment.rs")))
    {
        let mut at = 0usize;
        let mut found = false;
        while let Some((nl, name)) = next_top_level_fn(src, at) {
            at = nl + 1;
            if name != wanted {
                continue;
            }
            let end = src[nl..].find("\n}\n").map(|offset| nl + offset).unwrap_or(src.len());
            bodies.push_str(&src[nl..end]);
            bodies.push('\n');
            found = true;
            break;
        }
        assert!(found, "`{wanted}` not found in {origin} — the #5986 helper scan broke, not the doc");
    }
    bodies
}

/// The declaration of `apply_effect` in [`crate::fragment::SOURCES`], as
/// (offset of the leading newline, declaration line start).
fn apply_effect_declaration(src: &str) -> usize {
    let mut at = 0usize;
    while let Some((nl, name)) = next_top_level_fn(src, at) {
        if name == "apply_effect" {
            return nl;
        }
        at = nl + 1;
    }
    panic!("apply_effect's definition not found in fragment::SOURCES");
}

/// #3493 — `1d9a5041` (the #3250 fix) inserted `copied_transform` between
/// `apply_effect`'s nested-lock-safety doc block and `fn apply_effect`, so
/// Rust attached the whole lock contract to the three-line helper and left
/// `apply_effect` bare. Pin the adjacency: the residual-list block must be
/// the doc comment of the function it describes, with nothing between them.
#[test]
fn nested_lock_contract_documents_apply_effect_itself() {
    const FRAGMENT_RS: &str = crate::fragment::SOURCES;

    let contract = FRAGMENT_RS
        .find("**Nested-lock safety depends on exclusive scheduling.**")
        .expect("apply_effect's nested-lock residual list");
    let (next_item, next_name) = next_top_level_fn(FRAGMENT_RS, contract)
        .expect("a `fn` item follows the nested-lock doc block");
    let tail = FRAGMENT_RS;
    assert!(
        next_name == "apply_effect",
        "the nested-lock contract is attached to `{}`, not `apply_effect` — \
         a helper was inserted between the doc block and its item (#3493)",
        tail[next_item + 1..]
            .split('(')
            .next()
            .unwrap_or("<unknown>"),
    );
}

/// #3949 — the previous residual list said `apply_effect` ran while the
/// caller held the `QuestStageFragments`/`QuestStageState`/
/// `QuestObjectiveState` locks "for the whole cascade loop" and counted
/// "12 component-storage acquisitions". None of that survived the
/// guard-free rework: `QuestStageFragments` is cloned before any quest
/// resource is taken, the other two are one paired `resource_2_mut`
/// scoped to a single fragment, and the real acquisition set is larger.
///
/// The count is what rotted, so the count is what gets pinned. Every
/// storage/resource type `apply_effect` actually acquires must be named in
/// its own doc block — the next arm that adds one fails here instead of
/// silently ageing the inventory the Dim-6 checklist delegates to.
///
/// #5071 — "actually acquires" includes the one module the fragment bodies
/// call out to: the inline `crate::magic` arms' acquisitions are scanned
/// from `magic.rs` alongside the fragment sources, so a call-out one
/// module away can no longer hide a type from this inventory.
#[test]
fn the_nested_lock_residual_list_names_every_type_apply_effect_acquires() {
    const FRAGMENT_RS: &str = crate::fragment::SOURCES;
    const MAGIC_RS: &str = include_str!("../magic.rs");

    let contract_start = FRAGMENT_RS
        .find("**Nested-lock safety depends on exclusive scheduling.**")
        .expect("apply_effect's nested-lock residual list");
    let body_start = apply_effect_declaration(FRAGMENT_RS);
    let doc = &FRAGMENT_RS[contract_start..body_start];
    let mut bodies = apply_effect_family_bodies(FRAGMENT_RS);
    bodies.push_str(&magic_callout_bodies(MAGIC_RS));
    // #5986 — and the helpers one hop out in the fragment sources / equipment.rs.
    bodies.push_str(&nested_helper_bodies(FRAGMENT_RS, include_str!("../equipment.rs")));
    let body = bodies.as_str();

    // `.query_mut::<crate::HorseTetherState>()` -> `HorseTetherState`.
    // Needle composed at runtime so this test's own source cannot be what
    // the scan finds.
    let acquire = format!("{}{}", "::", "<");
    let mut acquired: Vec<String> = Vec::new();
    for (index, _) in body.match_indices(&acquire) {
        let before = &body[..index];
        if !(before.ends_with("query")
            || before.ends_with("query_mut")
            || before.ends_with("resource")
            || before.ends_with("resource_mut")
            || before.ends_with("try_resource")
            || before.ends_with("try_resource_mut")
            || before.ends_with("get")
            || before.ends_with("get_mut")
            // #5986 — the forms the scan used to miss: presence tests and the
            // paired acquisitions, whose turbofish names several types.
            || before.ends_with("has")
            || before.ends_with("query_2_mut_mut")
            || before.ends_with("resource_2_mut"))
        {
            continue;
        }
        let rest = &body[index + acquire.len()..];
        let Some(close) = rest.find('>') else {
            continue;
        };
        for path in rest[..close].split(',').map(str::trim) {
            if path.is_empty() || path.contains(' ') {
                continue;
            }
            let short = path.rsplit("::").next().unwrap_or(path).to_owned();
            if !acquired.contains(&short) {
                acquired.push(short);
            }
        }
    }
    assert!(
        acquired.len() > 10,
        "the acquisition scan found only {acquired:?} — the extraction broke, \
         not the doc"
    );

    let missing: Vec<&String> = acquired.iter().filter(|ty| !doc.contains(*ty)).collect();
    assert!(
        missing.is_empty(),
        "apply_effect acquires {missing:?} without naming them in its own \
         nested-lock residual list — that list is the inventory the Dim-6 \
         audit checklist delegates to, so an unlisted acquisition is \
         invisible to it (#3949)"
    );

    // #5986 — the false claim the inventory carried: the continuation queue is
    // written under these guards (`apply_effects`, `budget::defer`), and the doc
    // must not call the list exhaustive over a VM whose natives it cannot name.
    assert!(
        !doc.contains("is *not* in this list"),
        "FragmentExecutionQueue is taken under the paired guards (apply_effects / defer)"
    );
    assert!(
        !doc.contains("this list still inventories every lock"),
        "the list cannot inventory the VM's natives; the invariant is exclusive scheduling"
    );
    assert!(doc.contains("The Papyrus VM runs under these same guards"));

    // The specific stale claim, which no acquisition scan can catch: the
    // fragment table is cloned, never held.
    assert!(
        !doc.contains("holds the `QuestStageFragments`"),
        "`QuestStageFragments` is cloned before any quest resource is taken \
         (`quest_fragment_dispatch_system`), so it is never held across this \
         function (#3949)"
    );
}

/// #3935 — a long run of *sequential* `Effect::Conditional`s must dispatch
/// iteratively.
///
/// `MAX_CONDITIONAL_DEPTH` (#3279) bounds conditional *nesting* at lowering,
/// not how many siblings a fragment body may hold, and a `.pex` function may
/// carry 65 535 instructions — an empty `If Self.GetStageDone(n)` costs two.
/// The old dispatch recursed once per sibling and cloned the whole remaining
/// program at each one (`branch ++ effects[index + 1..]`), so N siblings cost
/// N stack frames and ~N^2/2 live `Effect` clones. Sized well past any
/// plausible authored fragment but far below what would abort the test
/// process, so this pins the shape rather than merely the survival: with the
/// recursion restored it is a debug-build stack overflow.
#[test]
fn apply_effects_dispatches_thousands_of_sequential_conditionals() {
    const SIBLINGS: u16 = 4_000;

    let world = World::new();
    let effects: Vec<Effect> = (0..SIBLINGS)
        .map(|k| Effect::Conditional {
            // `SelfRef` always resolves, and stage 0 is not done, so every
            // sibling takes its `else` branch — the arm that used to build
            // the flattened tail.
            guards: vec![FragmentGuard::StageDone {
                quest: QuestRef::SelfRef,
                stage: 0,
                done: true,
            }],
            then_effects: Vec::new(),
            else_effects: vec![Effect::SetObjectiveDisplayed {
                quest: QuestRef::SelfRef,
                objective: k as i32,
                displayed: true,
            }],
        })
        .collect();

    let mut stages = QuestStageState::default();
    let mut objectives = QuestObjectiveState::default();
    let mut deferred = DeferredFragmentEffects::new(&world);
    apply_effects(
        &effects,
        Q,
        None,
        &world,
        &mut stages,
        &mut objectives,
        &mut deferred,
    );

    // Every sibling ran, in order, exactly once.
    for k in 0..SIBLINGS {
        assert!(
            objectives.get(Q, k as i32).displayed,
            "sibling {k}'s else branch must have run"
        );
    }
}

/// #4813 — the ledger is three-state. An untouched reference defers to its
/// authored "Initially Disabled" flag; a scripted `Enable()` overrides that
/// flag (and must, across the next cell load), and a scripted `Disable()`
/// overrides a live one.
#[test]
fn reference_enable_state_scripted_calls_override_the_authored_default() {
    let mut state = ReferenceEnableState::default();
    assert_eq!(state.override_for(0x10), None);
    assert!(!state.is_enabled_with_default(0x10, true));
    assert!(state.is_enabled_with_default(0x10, false));

    state.set_enabled(0x10, true);
    assert_eq!(state.override_for(0x10), Some(true));
    assert!(state.is_enabled_with_default(0x10, true), "Enable() beats 0x800");

    state.set_enabled(0x10, false);
    assert_eq!(state.override_for(0x10), Some(false));
    assert!(!state.is_enabled_with_default(0x10, false));
    assert!(!state.is_enabled(0x10));
}

/// A VMAD whose object properties bind `(name, form id)` pairs, for the
/// inventory-effect dispatch tests below.
fn object_property_vmad(
    properties: &[(&str, u32)],
) -> byroredux_plugin::esm::records::script_instance::ScriptInstanceData {
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };
    ScriptInstanceData {
        version: 5,
        object_format: 2,
        scripts: vec![ScriptInstance {
            name: "QF_Test".into(),
            status: 0,
            properties: properties
                .iter()
                .map(|&(name, form_id)| ScriptProperty {
                    name: name.into(),
                    status: 1,
                    value: PropertyValue::Object {
                        form_id,
                        alias: -1,
                    },
                })
                .collect(),
        }],
    }
}

fn run_stage_effects(
    world: &World,
    vmad: byroredux_plugin::esm::records::script_instance::ScriptInstanceData,
    effects: Vec<Effect>,
) {
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert_vmad(Q, vmad);
        frags.insert(Q, 10, effects);
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(world, Q, 10);
    quest_fragment_dispatch_system(world);
}

/// M47.2 — `SetLocRefType` records the ref type against the reference, the
/// `HasLocRefType` guard and the alias candidates see it, and a `None` ref type
/// clears it again.
#[test]
fn set_loc_ref_type_assigns_and_clears() {
    const SPAWNED: u32 = 0x0000_6A00;
    const BOSS: u32 = 0x0000_6A01;
    let mut world = fixture();
    let entity = spawn_with_form_id(&mut world, SPAWNED);
    let vmad = || object_property_vmad(&[("Spawned", SPAWNED), ("Boss", BOSS)]);
    run_stage_effects(
        &world,
        vmad(),
        vec![Effect::SetLocRefType {
            object: ObjectRef::Property("Spawned".into()),
            ref_type: Some(ObjectRef::Property("Boss".into())),
        }],
    );
    let spawned = crate::condition::entity_global_form_id(&world, entity).expect("a form id");
    assert!(crate::has_loc_ref_type(&world, entity, spawned, BOSS));
    assert!(!crate::has_loc_ref_type(&world, entity, spawned, BOSS + 1));
    let mut candidates = vec![(
        entity,
        crate::SceneAliasCandidate { reference_form_id: spawned, ..Default::default() },
    )];
    crate::loc_ref_types::apply_to_candidates(&world, &mut candidates);
    assert_eq!(candidates[0].1.location_ref_types, vec![BOSS]);
    world.resource_mut::<QuestStageState>().set_stage(Q, 0);
    run_stage_effects(
        &world,
        vmad(),
        vec![Effect::SetLocRefType {
            object: ObjectRef::Property("Spawned".into()),
            ref_type: None,
        }],
    );
    assert!(!crate::has_loc_ref_type(&world, entity, spawned, BOSS));
}

/// M47.2 — `Game.GetPlayer().AddItem(..)` reaches the player's inventory and
/// is seen by `OnItemAdded`.
#[test]
fn dispatch_add_item_on_the_player() {
    use crate::translate::compose::ObjectRef;
    use crate::translate::effects::ActorRef;
    use byroredux_core::ecs::components::Inventory;

    const GOLD: u32 = 0x0000_000F;
    let mut world = fixture();
    world.register::<Inventory>();
    let player = world.resource::<PapyrusPlayerEntity>().0;
    run_stage_effects(
        &world,
        object_property_vmad(&[("Gold001", GOLD)]),
        vec![Effect::AddItem {
            silent: false,
            container: ActorRef::Player,
            item: ObjectRef::Property("Gold001".into()),
            count: crate::translate::effects::ItemCount::Literal(25),
        }],
    );
    let inventory = world.get::<Inventory>(player).expect("created on demand");
    assert_eq!(
        inventory.items.iter().map(|s| (s.base_form_id, s.count)).collect::<Vec<_>>(),
        vec![(GOLD, 25)]
    );
    let events = world.get::<crate::ItemEventBatch>(player).expect("OnItemAdded batch");
    assert_eq!(events.0.len(), 1);
    assert!(events.0[0].added);
    assert_eq!((events.0[0].item_form_id, events.0[0].count), (GOLD, 25));
}

/// #5971 — an object property left unset in the VMAD is the null object
/// (form 0). The effect that names it must skip: it used to write a form-0
/// inventory row and an `OnItemAdded` for form 0, and a stage on a phantom
/// quest 0, all saved with the world.
#[test]
fn dispatch_skips_an_unset_object_property() {
    use crate::translate::compose::ObjectRef;
    use crate::translate::effects::ActorRef;
    use byroredux_core::ecs::components::Inventory;

    let mut world = fixture();
    world.register::<Inventory>();
    let player = world.resource::<PapyrusPlayerEntity>().0;
    run_stage_effects(
        &world,
        object_property_vmad(&[("Gold001", 0), ("OtherQuest", 0)]),
        vec![
            Effect::AddItem {
                silent: false,
                container: ActorRef::Player,
                item: ObjectRef::Property("Gold001".into()),
                count: crate::translate::effects::ItemCount::Literal(25),
            },
            Effect::SetStage {
                quest: QuestRef::Property("OtherQuest".into()),
                stage: 40,
            },
        ],
    );
    let stacks = world
        .get::<Inventory>(player)
        .map(|inventory| inventory.items.len())
        .unwrap_or(0);
    assert_eq!(stacks, 0, "AddItem of the null object adds nothing");
    assert!(
        world.get::<crate::ItemEventBatch>(player).is_none(),
        "and raises no OnItemAdded for form 0"
    );
    assert_eq!(
        world.resource::<QuestStageState>().get_stage(QuestFormId(0)),
        0,
        "SetStage on an unset Quest property creates no quest-0 entry"
    );
}

/// #5973 — `ValueExpr::Abs` is the magnitude: a damage amount lowered from
/// Skyrim's sign-discarding `DamageActorValue` damages by `|n|` even when the
/// run-time value is negative, and a restore takes back `|n|`.
#[test]
fn abs_amount_damages_and_restores_by_magnitude() {
    use crate::translate::compose::ObjectRef;
    use crate::translate::effects::{ActorRef, ActorValueArg, ValueExpr};
    use byroredux_core::ecs::components::ActorValues;

    const AV: u32 = 0x0000_05F0;
    let mut world = fixture();
    world.register::<ActorValues>();
    let player = world.resource::<PapyrusPlayerEntity>().0;
    let mut values = ActorValues::new();
    values.set_base(AV, 10.0);
    world.insert(player, values);
    let damage = |amount: f32, restore: bool| Effect::DamageActorValue {
        actor: ActorRef::Player,
        actor_value: ActorValueArg::Form(ObjectRef::Property("Stat".into())),
        amount: ValueExpr::Abs(Box::new(ValueExpr::Literal(amount))),
        restore,
    };
    run_stage_effects(
        &world,
        object_property_vmad(&[("Stat", AV)]),
        vec![damage(-4.0, false)],
    );
    let current = |world: &World| world.get::<ActorValues>(player).unwrap().current(AV);
    assert_eq!(current(&world), 6.0, "a negative amount still damages, by its magnitude");
    world.resource_mut::<QuestStageState>().set_stage(Q, 0);
    run_stage_effects(
        &world,
        object_property_vmad(&[("Stat", AV)]),
        vec![damage(-3.0, true)],
    );
    assert_eq!(current(&world), 9.0, "a negative restore still restores, by its magnitude");
}

/// M47.2 — `RemoveItem` takes across stacks first-authored first, zeroes
/// drained rows in place (equipment indices stay valid), releases a drained
/// row's equipment and instance, moves items to an other container when one
/// is named, and reports each side to the item-event batches.
#[test]
fn dispatch_remove_item_drains_releases_and_moves() {
    use crate::translate::compose::ObjectRef;
    use crate::translate::effects::ActorRef;
    use byroredux_core::ecs::components::{EquipmentSlots, Inventory, InventoryIndex, ItemStack};
    use byroredux_core::ecs::resources::{ItemInstance, ItemInstancePool};

    const GOLD: u32 = 0x0000_000F;
    const SWORD: u32 = 0x0001_2EB7;
    const CHEST_FORM: u32 = 0x0000_8000;

    let mut world = fixture();
    world.register::<Inventory>();
    world.register::<EquipmentSlots>();
    let mut pool = ItemInstancePool::new();
    let sword_instance = pool.allocate(ItemInstance::default());
    world.insert_resource(pool);
    let player = world.resource::<PapyrusPlayerEntity>().0;
    let chest = spawn_with_form_id(&mut world, CHEST_FORM);
    let mut sword = ItemStack::new(SWORD, 1);
    sword.instance = Some(sword_instance);
    world.insert(
        player,
        Inventory {
            items: vec![ItemStack::new(GOLD, 100), sword, ItemStack::new(GOLD, 30)],
        },
    );
    let mut slots = EquipmentSlots::new();
    slots.equip_weapon(InventoryIndex(1));
    world.insert(player, slots);

    let item = |name: &str| ObjectRef::Property(name.into());
    run_stage_effects(
        &world,
        object_property_vmad(&[("Gold001", GOLD), ("Sword", SWORD), ("Chest", CHEST_FORM)]),
        vec![
            // 100 from the first stack (drained), 20 from the third.
            Effect::RemoveItem {
                container: ActorRef::Player,
                item: item("Gold001"),
                count: crate::translate::effects::ItemCount::Literal(120),
                destination: None,
            },
            Effect::RemoveItem {
                container: ActorRef::Player,
                item: item("Sword"),
                count: crate::translate::effects::ItemCount::Literal(1),
                destination: None,
            },
            // More than is left: the remaining 10 move, no more.
            Effect::RemoveItem {
                container: ActorRef::Player,
                item: item("Gold001"),
                count: crate::translate::effects::ItemCount::Literal(50),
                destination: Some(ActorRef::Object(item("Chest"))),
            },
        ],
    );

    let inventory = world.get::<Inventory>(player).unwrap();
    assert_eq!(
        inventory.items.iter().map(|s| (s.base_form_id, s.count)).collect::<Vec<_>>(),
        vec![(GOLD, 0), (SWORD, 0), (GOLD, 0)],
        "rows are zeroed in place, never removed"
    );
    drop(inventory);
    assert_eq!(
        world.get::<EquipmentSlots>(player).unwrap().equipped_indices().count(),
        0,
        "the drained sword row's equipment is released"
    );
    assert!(
        world.resource::<ItemInstancePool>().get(sword_instance).is_none(),
        "the drained row's instance returns to the pool"
    );
    let chest_items = world.get::<Inventory>(chest).expect("destination created on demand");
    assert_eq!(
        chest_items.items.iter().map(|s| (s.base_form_id, s.count)).collect::<Vec<_>>(),
        vec![(GOLD, 10)]
    );
    drop(chest_items);
    let removed: Vec<(u32, u32, bool)> = world
        .get::<crate::ItemEventBatch>(player)
        .unwrap()
        .0
        .iter()
        .map(|t| (t.item_form_id, t.count, t.added))
        .collect();
    assert_eq!(removed, vec![(GOLD, 120, false), (SWORD, 1, false), (GOLD, 10, false)]);
    let added: Vec<(u32, u32, bool)> = world
        .get::<crate::ItemEventBatch>(chest)
        .unwrap()
        .0
        .iter()
        .map(|t| (t.item_form_id, t.count, t.added))
        .collect();
    assert_eq!(added, vec![(GOLD, 10, true)]);
}

/// M47.2 — a fragment's `kmyQuest.Property = literal` lands in
/// `QuestScriptVariables` under the backing name a `GetVMQuestVariable`
/// condition reads, once the quest guards have dropped.
#[test]
fn dispatch_set_quest_variable_is_read_by_get_vm_quest_variable() {
    use crate::quest_variables::QuestScriptVariables;
    use crate::translate::compose::QuestRef;
    use byroredux_plugin::esm::records::condition::{
        ComparisonOp, Condition, ConditionStringId, ConditionValue,
    };

    let mut world = fixture();
    world.insert_resource(QuestScriptVariables::default());
    run_stage_effects(
        &world,
        object_property_vmad(&[]),
        vec![Effect::SetQuestVariable {
            quest: QuestRef::SelfRef,
            variable: "GetInHjerimTovaPointer".into(),
            value: 1.0,
        }],
    );
    let condition = Condition {
        function_index: 629,
        comparator: ComparisonOp::Eq,
        comparand: ConditionValue::Literal(1.0),
        param_1: Q.0,
        param_2_text: Some(ConditionStringId::from_text("::GetInHjerimTovaPointer_var")),
        ..Default::default()
    };
    let player = world.resource::<PapyrusPlayerEntity>().0;
    assert!(crate::condition::evaluate(
        &vec![condition],
        &world,
        &crate::condition::ConditionContext::for_subject(player),
    ));
}

/// M47.2 — a spoken line's fragment resolves `ActorRef::Speaker` to the NPC
/// the dialogue caller names; without one the effect is skipped.
#[test]
fn spoken_fragment_resolves_the_speaker() {
    use crate::translate::compose::ObjectRef;
    use crate::translate::effects::ActorRef;
    use byroredux_core::ecs::components::Inventory;

    const GOLD: u32 = 0x0000_000F;
    let mut world = fixture();
    world.register::<Inventory>();
    let npc = world.spawn();
    let vmad = object_property_vmad(&[("Gold001", GOLD)]);
    let effects = vec![Effect::AddItem {
        silent: false,
        container: ActorRef::Speaker,
        item: ObjectRef::Property("Gold001".into()),
        count: crate::translate::effects::ItemCount::Literal(10),
    }];
    crate::apply_spoken_info_fragment(&world, &effects, Q, Some(&vmad), None);
    assert!(world.get::<Inventory>(npc).is_none(), "no speaker, no effect");
    crate::apply_spoken_info_fragment(&world, &effects, Q, Some(&vmad), Some(npc));
    let inventory = world.get::<Inventory>(npc).expect("the speaker received the gold");
    assert_eq!(inventory.items[0].base_form_id, GOLD);
    assert_eq!(inventory.items[0].count, 10);
}

/// M47.2 — every widened guard kind reads its live state at dispatch, and a
/// quest-variable guard sees a write made earlier in the same fragment.
#[test]
fn widened_fragment_guards_read_live_state() {
    use crate::quest_variables::QuestScriptVariables;
    use crate::translate::compose::{ObjectRef, QuestRef};
    use crate::translate::effects::GuardComparison;

    const RANSOM: u32 = 0x0000_9001;
    let mut world = fixture();
    world.insert_resource(QuestScriptVariables::default());
    world.insert_resource(crate::Globals::default());
    world.resource_mut::<crate::Globals>().set(RANSOM, 3.0);
    world
        .resource_mut::<crate::quest_stages::QuestObjectiveState>()
        .set_displayed(Q, 10, true);

    let probe = |name: &str, guard: FragmentGuard| Effect::Conditional {
        guards: vec![guard],
        then_effects: vec![Effect::SetQuestVariable {
            quest: QuestRef::SelfRef,
            variable: name.into(),
            value: 1.0,
        }],
        else_effects: vec![Effect::SetQuestVariable {
            quest: QuestRef::SelfRef,
            variable: name.into(),
            value: -1.0,
        }],
    };
    run_stage_effects(
        &world,
        object_property_vmad(&[("Ransom", RANSOM)]),
        vec![
            Effect::SetQuestVariable {
                quest: QuestRef::SelfRef,
                variable: "Flag".into(),
                value: 2.0,
            },
            probe("Running", FragmentGuard::QuestRunning { quest: QuestRef::SelfRef, running: true }),
            probe(
                "StageGe",
                FragmentGuard::Stage {
                    quest: QuestRef::SelfRef,
                    comparison: GuardComparison::Ge,
                    value: 10.0,
                },
            ),
            probe(
                "StageLt",
                FragmentGuard::Stage {
                    quest: QuestRef::SelfRef,
                    comparison: GuardComparison::Lt,
                    value: 10.0,
                },
            ),
            probe(
                "Displayed",
                FragmentGuard::ObjectiveDisplayed { quest: QuestRef::SelfRef, objective: 10, expected: true },
            ),
            probe(
                "Completed",
                FragmentGuard::ObjectiveCompleted { quest: QuestRef::SelfRef, objective: 10, expected: true },
            ),
            probe(
                "Global",
                FragmentGuard::Global {
                    global: ObjectRef::Property("Ransom".into()),
                    comparison: GuardComparison::Eq,
                    value: 3.0,
                },
            ),
            probe(
                "SameFragmentWrite",
                FragmentGuard::QuestVariable {
                    quest: QuestRef::SelfRef,
                    variable: "Flag".into(),
                    comparison: GuardComparison::Eq,
                    value: 2.0,
                },
            ),
            // M47.2 — a disjunction holds when one alternative does.
            probe(
                "AnyHolds",
                FragmentGuard::Any(vec![
                    vec![FragmentGuard::ObjectiveCompleted {
                        quest: QuestRef::SelfRef,
                        objective: 10,
                        expected: true,
                    }],
                    vec![
                        FragmentGuard::QuestRunning { quest: QuestRef::SelfRef, running: true },
                        FragmentGuard::ObjectiveDisplayed {
                            quest: QuestRef::SelfRef,
                            objective: 10,
                            expected: true,
                        },
                    ],
                ]),
            ),
            probe(
                "AnyFails",
                FragmentGuard::Any(vec![
                    vec![FragmentGuard::ObjectiveCompleted {
                        quest: QuestRef::SelfRef,
                        objective: 10,
                        expected: true,
                    }],
                    vec![FragmentGuard::QuestRunning { quest: QuestRef::SelfRef, running: false }],
                ]),
            ),
        ],
    );
    let vars = world.resource::<QuestScriptVariables>();
    let read = |name| vars.get(Q, QuestScriptVariables::property_key(name));
    // `run_stage_effects` sets stage 10, which starts the quest.
    assert_eq!(read("Running"), Some(1.0));
    assert_eq!(read("StageGe"), Some(1.0));
    assert_eq!(read("StageLt"), Some(-1.0));
    assert_eq!(read("Displayed"), Some(1.0));
    assert_eq!(read("Completed"), Some(-1.0));
    assert_eq!(read("Global"), Some(1.0));
    assert_eq!(read("SameFragmentWrite"), Some(1.0));
    assert_eq!(read("AnyHolds"), Some(1.0));
    assert_eq!(read("AnyFails"), Some(-1.0));
}

/// M47.2 — `SendStoryEvent` attaches an `SCPT` marker (after the guards)
/// carrying the keyword, the location and the integers; with no `akRef1` it
/// rides the player but reports no R1.
#[test]
fn send_story_event_raises_a_script_event() {
    const KEYWORD: u32 = 0x0004_1E8B;
    const LOCATION: u32 = 0x0001_8A56;
    let world = fixture();
    let player = world.resource::<crate::papyrus_demo::PapyrusPlayerEntity>().0;
    run_stage_effects(
        &world,
        object_property_vmad(&[("SomeKeyword", KEYWORD), ("SomeLocation", LOCATION)]),
        vec![Effect::SendStoryEvent {
            keyword: ObjectRef::Property("SomeKeyword".into()),
            location: Some(ObjectRef::Property("SomeLocation".into())),
            references: [None, None],
            values: [5, 6],
        }],
    );
    let pending = world.resource::<crate::story_manager::PendingScriptStoryEvents>();
    assert_eq!(pending.0.len(), 1);
    let event = pending.0[0];
    assert_eq!(event.reference_1, player);
    assert_eq!(&event.mnemonic, b"SCPT");
    let slots = event.slots();
    assert_eq!(slots.keyword, Some(KEYWORD));
    assert_eq!(slots.location_1, Some(LOCATION));
    assert_eq!(slots.values, [5, 6]);
    assert_eq!(slots.reference_1, None, "no akRef1");
}

/// M47.2 — `AddToMap` records the marker once the guards drop, and a guard
/// later in the same fragment already sees it.
#[test]
fn add_to_map_records_the_marker() {
    use crate::QuestScriptVariables;
    const MARKER: u32 = 0x0000_6900;
    let mut world = fixture();
    world.insert_resource(QuestScriptVariables::default());
    run_stage_effects(
        &world,
        object_property_vmad(&[("LairMarker", MARKER)]),
        vec![
            Effect::AddToMap {
                marker: ObjectRef::Property("LairMarker".into()),
                fast_travel: true,
            },
            Effect::Conditional {
                guards: vec![FragmentGuard::MapMarkerFastTravel {
                    marker: ObjectRef::Property("LairMarker".into()),
                    expected: true,
                }],
                then_effects: vec![Effect::SetQuestVariable {
                    quest: QuestRef::SelfRef,
                    variable: "Seen".into(),
                    value: 1.0,
                }],
                else_effects: vec![],
            },
        ],
    );
    assert!(crate::map_marker_visible(&world, MARKER));
    assert!(crate::map_marker_fast_travel(&world, MARKER));
    assert_eq!(
        world
            .resource::<QuestScriptVariables>()
            .get(Q, QuestScriptVariables::property_key("Seen")),
        Some(1.0)
    );
}

/// M47.2 — `SetCleared` records the location once the guards drop, and an
/// `IsCleared` guard later in the same fragment already sees it.
#[test]
fn set_cleared_clears_the_location() {
    use crate::QuestScriptVariables;
    const LOCATION: u32 = 0x0001_8EE9;
    let mut world = fixture();
    world.insert_resource(QuestScriptVariables::default());
    world.insert_resource(crate::LocationClearState::default());
    let dungeon = || ObjectRef::Property("Dungeon".into());
    run_stage_effects(
        &world,
        object_property_vmad(&[("Dungeon", LOCATION)]),
        vec![
            Effect::SetLocationCleared {
                location: dungeon(),
                cleared: true,
            },
            Effect::Conditional {
                guards: vec![FragmentGuard::LocationCleared {
                    location: dungeon(),
                    expected: true,
                }],
                then_effects: vec![Effect::SetQuestVariable {
                    quest: QuestRef::SelfRef,
                    variable: "Seen".into(),
                    value: 1.0,
                }],
                else_effects: vec![],
            },
        ],
    );
    assert!(crate::location_cleared(&world, LOCATION));
    assert_eq!(
        world
            .resource::<QuestScriptVariables>()
            .get(Q, QuestScriptVariables::property_key("Seen")),
        Some(1.0)
    );
}

/// M47.2 — a decompiled quest-property receiver (`::Other_var`) resolves
/// through its authored VMAD name, like an `ObjectRef` does.
#[test]
fn a_backing_variable_quest_property_resolves_through_its_authored_name() {
    use crate::quest_variables::QuestScriptVariables;
    use crate::translate::compose::QuestRef;
    const OTHER: QuestFormId = QuestFormId(0x0004_E6C0);
    let mut world = fixture();
    world.insert_resource(QuestScriptVariables::default());
    run_stage_effects(
        &world,
        object_property_vmad(&[("pTG04PQS", OTHER.0)]),
        vec![Effect::SetQuestVariable {
            quest: QuestRef::Property("::pTG04PQS_var".into()),
            variable: "pTG04PostChoice".into(),
            value: 3.0,
        }],
    );
    assert_eq!(
        world
            .resource::<QuestScriptVariables>()
            .get(OTHER, QuestScriptVariables::property_key("pTG04PostChoice")),
        Some(3.0)
    );
}

/// #5913 — a scripted add to the player raises the pickup notice unless the
/// script passed `abSilent = true`; the notice is worded like the engine's own
/// pickup ("Added <name> (<count>)") and names the item from the load order.
#[test]
fn scripted_adds_to_the_player_raise_a_pickup_notice_unless_silent() {
    use crate::translate::compose::ObjectRef;
    use crate::translate::effects::{ActorRef, ItemCount};
    const SWORD: u32 = 0x0001_2EB7;
    const POTION: u32 = 0x0003_3A00;
    let mut world = fixture();
    crate::messages::register(&mut world);
    let add = |item: &str, count, silent| Effect::AddItem {
        container: ActorRef::Player,
        item: ObjectRef::Property(item.into()),
        count: ItemCount::Literal(count),
        silent,
    };
    run_stage_effects(
        &world,
        object_property_vmad(&[("Sword", SWORD), ("Potion", POTION)]),
        vec![
            add("Sword", 1, false),
            add("Potion", 3, false),
            add("Potion", 9, true),
            Effect::PlayerCaps { delta: 100, silent: false },
            Effect::PlayerCaps { delta: 7, silent: true },
            // Removing never notifies.
            Effect::PlayerCaps { delta: -5, silent: false },
        ],
    );
    let texts: Vec<String> = world
        .resource::<crate::PendingScriptMessages>()
        .0
        .iter()
        .map(|message| message.text.clone())
        .collect();
    assert_eq!(
        texts,
        [
            "Added Item 00012EB7",
            "Added Item 00033A00 (3)",
            "Added Item 0000000F (100)",
        ],
        "silent adds and removals raise nothing"
    );
}

/// M47.2 — `PlayerCaps` adds / removes caps (form `0xF`) on the player and
/// `RewardPlayerXp` queues the grant.
#[test]
fn player_caps_and_reward_xp_reach_the_player() {
    use byroredux_core::ecs::components::Inventory;
    let world = fixture();
    let player = world.resource::<crate::papyrus_demo::PapyrusPlayerEntity>().0;
    run_stage_effects(
        &world,
        object_property_vmad(&[]),
        vec![
            Effect::PlayerCaps { delta: 100, silent: false },
            Effect::PlayerCaps { delta: -30, silent: false },
            Effect::RewardPlayerXp {
                amount: crate::translate::effects::ItemCount::Literal(50),
                direct: false,
            },
        ],
    );
    let caps: u32 = world
        .get::<Inventory>(player)
        .unwrap()
        .items
        .iter()
        .filter(|stack| stack.base_form_id == 0xF)
        .map(|stack| stack.count)
        .sum();
    assert_eq!(caps, 70);
    assert_eq!(
        world.resource::<crate::story_manager::PendingPlayerXp>().0,
        vec![(50, false)]
    );
}

/// M47.2 — `SetReferenceVariable` writes the loaded reference's
/// `ScriptVariables`, which the `GetVMScriptVariable` condition reads.
#[test]
fn set_reference_variable_reaches_get_vm_script_variable() {
    use byroredux_plugin::esm::records::condition::{
        ComparisonOp, Condition, ConditionStringId, ConditionValue, RunOn,
    };
    const TRAP_FORM: u32 = 0x0009_0C23;
    let mut world = fixture();
    let trap = spawn_with_form_id(&mut world, TRAP_FORM);
    run_stage_effects(
        &world,
        object_property_vmad(&[("PlateTrap", TRAP_FORM)]),
        vec![Effect::SetReferenceVariable {
            object: ObjectRef::Property("::PlateTrap_var".into()),
            variable: "::plateTriggered_var".into(),
            value: 1.0,
        }],
    );
    let condition = Condition {
        function_index: 630,
        comparator: ComparisonOp::Eq,
        comparand: ConditionValue::Literal(1.0),
        param_1: TRAP_FORM,
        param_2_text: Some(ConditionStringId::from_text("::plateTriggered_var")),
        run_on: RunOn::Subject,
        ..Default::default()
    };
    assert!(crate::condition::evaluate(
        &vec![condition],
        &world,
        &crate::condition::ConditionContext::for_subject(trap),
    ));
}

/// M47.2 — a `SetLocal` value feeds a later `Compare` guard in the same run.
#[test]
fn set_local_feeds_a_compare_guard() {
    use crate::translate::effects::{ArithOp, GuardComparison, ValueExpr};
    use crate::QuestScriptVariables;
    let mut world = fixture();
    world.insert_resource(QuestScriptVariables::default());
    run_stage_effects(
        &world,
        object_property_vmad(&[]),
        vec![
            Effect::SetLocal {
                local: "n".into(),
                value: ValueExpr::Arith(
                    Box::new(ValueExpr::Literal(2.0)),
                    ArithOp::Mul,
                    Box::new(ValueExpr::Literal(3.0)),
                ),
            },
            Effect::Conditional {
                guards: vec![FragmentGuard::Compare {
                    left: ValueExpr::Local("n".into()),
                    comparison: GuardComparison::Eq,
                    right: ValueExpr::Literal(6.0),
                }],
                then_effects: vec![Effect::SetQuestVariable {
                    quest: QuestRef::SelfRef,
                    variable: "Six".into(),
                    value: 1.0,
                }],
                else_effects: vec![],
            },
        ],
    );
    assert_eq!(
        world
            .resource::<QuestScriptVariables>()
            .get(Q, QuestScriptVariables::property_key("Six")),
        Some(1.0)
    );
}

/// M47.2 — `CopyQuestVariable` copies a value written earlier in the same
/// fragment, and an unset source leaves the target untouched.
#[test]
fn copy_quest_variable_reads_the_pending_write() {
    use crate::quest_variables::QuestScriptVariables;
    use crate::translate::compose::QuestRef;
    let mut world = fixture();
    world.insert_resource(QuestScriptVariables::default());
    run_stage_effects(
        &world,
        object_property_vmad(&[]),
        vec![
            Effect::SetQuestVariable {
                quest: QuestRef::SelfRef,
                variable: "::CurrentSong_var".into(),
                value: 29.0,
            },
            Effect::CopyQuestVariable {
                quest: QuestRef::SelfRef,
                from: "::CurrentSong_var".into(),
                to: "::LastSong01_var".into(),
            },
            Effect::CopyQuestVariable {
                quest: QuestRef::SelfRef,
                from: "::NeverSet_var".into(),
                to: "::LastSong02_var".into(),
            },
        ],
    );
    let read = |name| {
        world
            .resource::<QuestScriptVariables>()
            .get(Q, QuestScriptVariables::property_key(name))
    };
    assert_eq!(read("LastSong01"), Some(29.0));
    assert_eq!(read("LastSong02"), None);
}

/// M47.2 — a `QuestScope` runs against its quest: the quest is the context
/// and its own VMAD (not the fragment's) binds the properties.
#[test]
fn a_quest_scope_runs_against_its_quests_vmad() {
    use crate::translate::compose::QuestRef;
    const RADIO_MARKER: u32 = 0x000E_F21D;
    let world = fixture();
    let mut instances = HashMap::new();
    instances.insert(Q, object_property_vmad(&[("RadioMarker", RADIO_MARKER)]));
    {
        let mut world = world;
        world.insert_resource(super::QuestScriptInstances(std::sync::Arc::new(instances)));
        world
            .resource_mut::<crate::fragment::ReferenceEnableState>()
            .set_enabled(RADIO_MARKER, false);
        run_stage_effects(
            &world,
            // The INFO's own VMAD does not carry the property.
            object_property_vmad(&[]),
            vec![Effect::QuestScope {
                bindings: Vec::new(),
                quest: QuestRef::OwningQuest,
                effects: vec![Effect::Enable {
                    object: ObjectRef::Property("RadioMarker".into()),
                    fade_in: false,
                }],
            }],
        );
        assert!(super::reference_enabled(&world, RADIO_MARKER));
    }
}

/// M47.2 — `IsDisabled` / `IsEnabled` guards read a script's override,
/// including one earlier in the same fragment, else the authored state.
#[test]
fn reference_enabled_guard_reads_scripts_then_the_authored_state() {
    use crate::QuestScriptVariables;
    const STARTS_DISABLED: u32 = 0x0002_0001;
    const STARTS_ENABLED: u32 = 0x0002_0002;
    let mut world = fixture();
    world.insert_resource(QuestScriptVariables::default());
    world.insert_resource(super::AuthoredDisabledReferences {
        initially_disabled: [STARTS_DISABLED].into_iter().collect(),
        enable_parents: Default::default(),
    });
    let enabled = |name: &str, expected| FragmentGuard::ReferenceEnabled {
        object: ObjectRef::Property(name.into()),
        expected,
    };
    let mark = |variable: &str| Effect::SetQuestVariable {
        quest: QuestRef::SelfRef,
        variable: variable.into(),
        value: 1.0,
    };
    run_stage_effects(
        &world,
        object_property_vmad(&[("Hidden", STARTS_DISABLED), ("Shown", STARTS_ENABLED)]),
        vec![
            Effect::Conditional {
                guards: vec![enabled("Hidden", false), enabled("Shown", true)],
                then_effects: vec![mark("Authored")],
                else_effects: vec![],
            },
            Effect::Enable {
                object: ObjectRef::Property("Hidden".into()),
                fade_in: false,
            },
            Effect::Conditional {
                guards: vec![enabled("Hidden", true)],
                then_effects: vec![mark("Pending")],
                else_effects: vec![],
            },
        ],
    );
    let read = |name| {
        world
            .resource::<QuestScriptVariables>()
            .get(Q, QuestScriptVariables::property_key(name))
    };
    assert_eq!(read("Authored"), Some(1.0));
    assert_eq!(read("Pending"), Some(1.0));
    assert!(super::reference_enabled(&world, STARTS_DISABLED), "the Enable() landed");
}

/// M47.2 — `SetEssential` on an ActorBase property records the override
/// once the guards drop, clearing the protected flag.
#[test]
fn set_essential_records_the_base_override() {
    const BASE: u32 = 0x0001_3478;
    let world = fixture();
    run_stage_effects(
        &world,
        object_property_vmad(&[("EmperorBase", BASE)]),
        vec![Effect::SetBaseProtection {
            base: crate::translate::effects::ActorBaseRef::Form(ObjectRef::Property(
                "EmperorBase".into(),
            )),
            protected: false,
            value: true,
        }],
    );
    assert_eq!(
        world.resource::<crate::ActorBaseProtectionOverrides>().0.get(&BASE),
        Some(&(Some(true), Some(false)))
    );
}

/// M47.2 — `BlockActivation` records the reference in the activation-block
/// ledger once the quest-state guards drop, and an unblock clears it.
#[test]
fn block_activation_records_the_reference() {
    const DOOR: u32 = 0x0000_6800;
    let mut world = fixture();
    let door = spawn_with_form_id(&mut world, DOOR);
    run_stage_effects(
        &world,
        object_property_vmad(&[("SomeDoor", DOOR)]),
        vec![Effect::BlockActivation {
            object: ObjectRef::Property("SomeDoor".into()),
            blocked: true,
        }],
    );
    assert!(crate::default_activation_blocked(&world, door));
    world.resource_mut::<QuestStageState>().set_stage(Q, 20);
    run_stage_effects(
        &world,
        object_property_vmad(&[("SomeDoor", DOOR)]),
        vec![Effect::BlockActivation {
            object: ObjectRef::Property("SomeDoor".into()),
            blocked: false,
        }],
    );
    assert!(!crate::default_activation_blocked(&world, door));
}

/// M47.2 — Skyrim's `SetAV("Name", v)` resolves the AVIF through the
/// editor-ID table and sets the base value; an unknown name skips.
#[test]
fn named_actor_value_writes_resolve_through_the_avif_names() {
    use crate::translate::effects::ActorRef;
    use byroredux_core::ecs::components::ActorValues;
    use byroredux_plugin::esm::records::AvifRecord;
    let mut world = fixture();
    world.register::<ActorValues>();
    let player = world.spawn();
    world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(player));
    let mut index = byroredux_plugin::esm::records::EsmIndex::default();
    index.actor_values.insert(
        0x5F0,
        AvifRecord {
            form_id: 0x5F0,
            editor_id: "AVVariable06".into(),
            ..Default::default()
        },
    );
    crate::install_actor_value_names(&mut world, &index);
    run_stage_effects(
        &world,
        object_property_vmad(&[]),
        vec![
            Effect::SetActorValueNamed {
                actor: ActorRef::Player,
                name: "Variable06".into(),
                value: crate::translate::effects::ValueExpr::Literal(3.0),
            },
            Effect::SetActorValueNamed {
                actor: ActorRef::Player,
                name: "NoSuchValue".into(),
                value: crate::translate::effects::ValueExpr::Literal(9.0),
            },
        ],
    );
    let values = world.get::<ActorValues>(player).expect("created");
    assert_eq!(values.current(0x5F0), 3.0);
    assert_eq!(values.len(), 1, "the unknown name wrote nothing");
}

/// M47.2 — a quest-variable increment builds on the variable's value,
/// including a write earlier in the same fragment.
#[test]
fn quest_variable_increments_build_on_the_current_value() {
    use crate::QuestScriptVariables;
    let mut world = fixture();
    world.insert_resource(QuestScriptVariables::default());
    run_stage_effects(
        &world,
        object_property_vmad(&[]),
        vec![
            Effect::ModQuestVariable {
                quest: QuestRef::SelfRef,
                variable: "Count".into(),
                delta: 1.0,
            },
            Effect::ModQuestVariable {
                quest: QuestRef::SelfRef,
                variable: "Count".into(),
                delta: 2.0,
            },
        ],
    );
    let vars = world.resource::<QuestScriptVariables>();
    assert_eq!(vars.get(Q, QuestScriptVariables::property_key("Count")), Some(3.0));
}

/// M47.2 — `ForceRefTo` records the target's reference FormID against the
/// alias the property names (quest + alias index), and `Clear` records the
/// clear.
#[test]
fn force_ref_to_and_clear_record_alias_overrides() {
    use crate::translate::effects::ActorRef;
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };
    const OTHER_QUEST: u32 = 0x0000_7777;
    let world = fixture();
    let vmad = ScriptInstanceData {
        version: 5,
        object_format: 2,
        scripts: vec![ScriptInstance {
            name: "QF_Test".into(),
            status: 0,
            properties: vec![
                ScriptProperty {
                    name: "Alias_Giver".into(),
                    status: 1,
                    value: PropertyValue::Object {
                        form_id: OTHER_QUEST,
                        alias: 3,
                    },
                },
                ScriptProperty {
                    name: "Alias_Own".into(),
                    status: 1,
                    value: PropertyValue::Object { form_id: 0, alias: 4 },
                },
            ],
        }],
    };
    run_stage_effects(
        &world,
        vmad,
        vec![
            Effect::ForceAlias {
                alias: ObjectRef::Property("Alias_Giver".into()),
                target: Some(ActorRef::Player),
            },
            Effect::ForceAlias {
                alias: ObjectRef::Property("Alias_Own".into()),
                target: None,
            },
        ],
    );
    let overrides = world.resource::<crate::AliasOverrides>();
    assert_eq!(
        overrides.get(QuestFormId(OTHER_QUEST), 3),
        Some(crate::AliasOverride::Forced(0x14))
    );
    assert_eq!(overrides.get(Q, 4), Some(crate::AliasOverride::Cleared));
}

/// M47.2 — `ForceLocationTo` records the location against the location alias the
/// property names.
#[test]
fn force_location_to_records_a_location_override() {
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };
    const PLACE: u32 = 0x0001_5555;
    let world = fixture();
    let object = |name: &str, form_id: u32, alias: i16| ScriptProperty {
        name: name.into(),
        status: 1,
        value: PropertyValue::Object { form_id, alias },
    };
    let vmad = ScriptInstanceData {
        version: 5,
        object_format: 2,
        scripts: vec![ScriptInstance {
            name: "QF_Test".into(),
            status: 0,
            properties: vec![object("Alias_Site", 0, 6), object("Hold", PLACE, -1)],
        }],
    };
    run_stage_effects(
        &world,
        vmad,
        vec![Effect::ForceLocationTo {
            alias: ObjectRef::Property("Alias_Site".into()),
            location: ObjectRef::Property("Hold".into()),
        }],
    );
    assert_eq!(
        world.resource::<crate::AliasOverrides>().get(Q, 6),
        Some(crate::AliasOverride::ForcedLocation(PLACE))
    );
}

/// M47.2 — guards over actor state (death, perks, items), scene playback
/// and the player's current location read the live world.
#[test]
fn actor_scene_and_location_guards_read_the_world() {
    use crate::translate::effects::{ActorRef, GuardComparison};
    use crate::QuestScriptVariables;
    use byroredux_core::character::Perks;
    use byroredux_core::ecs::components::{Dead, ItemStack};
    const VICTIM: u32 = 0x0000_6600;
    const PERK: u32 = 0x0000_6601;
    const GOLD: u32 = 0x0000_000F;
    const CITY: u32 = 0x0001_3168;
    const WHITERUN: u32 = 0x0001_8A56;
    let mut world = fixture();
    world.insert_resource(QuestScriptVariables::default());
    world.register::<Perks>();
    world.register::<Dead>();
    let victim = spawn_with_form_id(&mut world, VICTIM);
    world.insert(victim, Dead);
    let player = world.spawn();
    world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(player));
    let mut perks = Perks::default();
    perks.set_rank(PERK, 1);
    world.insert(player, perks);
    let mut inventory = Inventory::new();
    inventory.push(ItemStack::new(GOLD, 60));
    inventory.push(ItemStack::new(GOLD, 50));
    world.insert(player, inventory);
    let mut index = byroredux_plugin::esm::records::EsmIndex::default();
    index.locations.insert(
        WHITERUN,
        byroredux_plugin::esm::records::LctnRecord {
            form_id: WHITERUN,
            keywords: vec![CITY],
            ..Default::default()
        },
    );
    crate::location::install_location_table(&mut world, &index);
    world.resource_mut::<crate::location::PlayerLocation>().0 = Some(WHITERUN);
    const SCENE: u32 = 0x0000_6700;
    crate::install_scene_records(
        &mut world,
        [byroredux_plugin::esm::records::ScenRecord {
            form_id: SCENE,
            ..Default::default()
        }],
    );

    let probe = |name: &str, guard: FragmentGuard| Effect::Conditional {
        guards: vec![guard],
        then_effects: vec![Effect::SetQuestVariable {
            quest: QuestRef::SelfRef,
            variable: name.into(),
            value: 1.0,
        }],
        else_effects: vec![Effect::SetQuestVariable {
            quest: QuestRef::SelfRef,
            variable: name.into(),
            value: -1.0,
        }],
    };
    let property = |name: &str| ObjectRef::Property(name.into());
    run_stage_effects(
        &world,
        object_property_vmad(&[
            ("Victim", VICTIM),
            ("SomePerk", PERK),
            ("Gold001", GOLD),
            ("LocTypeCity", CITY),
            ("SomeScene", SCENE),
        ]),
        vec![
            probe(
                "Dead",
                FragmentGuard::ActorDead {
                    actor: ActorRef::Object(property("Victim")),
                    dead: true,
                },
            ),
            probe(
                "Perk",
                FragmentGuard::HasPerk {
                    actor: ActorRef::Player,
                    perk: property("SomePerk"),
                    expected: true,
                },
            ),
            probe(
                "Rich",
                FragmentGuard::ItemCount {
                    container: ActorRef::Player,
                    item: property("Gold001"),
                    comparison: GuardComparison::Ge,
                    value: 100.0,
                },
            ),
            probe(
                "InCity",
                FragmentGuard::LocationKeyword {
                    actor: ActorRef::Player,
                    keyword: property("LocTypeCity"),
                    expected: true,
                },
            ),
            probe(
                "SceneIdle",
                FragmentGuard::ScenePlaying {
                    scene: property("SomeScene"),
                    playing: false,
                },
            ),
            probe(
                "NotCompleted",
                FragmentGuard::QuestCompleted {
                    quest: QuestRef::SelfRef,
                    completed: true,
                },
            ),
        ],
    );
    let vars = world.resource::<QuestScriptVariables>();
    let read = |name| vars.get(Q, QuestScriptVariables::property_key(name));
    assert_eq!(read("Dead"), Some(1.0));
    assert_eq!(read("Perk"), Some(1.0));
    assert_eq!(read("Rich"), Some(1.0), "60 + 50 gold across two stacks");
    assert_eq!(read("InCity"), Some(1.0));
    assert_eq!(read("SceneIdle"), Some(1.0), "a scene nobody started");
    assert_eq!(read("NotCompleted"), Some(-1.0));
}

/// M47.2 — a faction change edits the live `FactionRanks` and lands in the
/// saved ledger under the reference FormID (PlayerRef for the player), so the
/// spawn / load re-derivation can replay it.
#[test]
fn dispatch_change_faction_edits_ranks_and_records_the_ledger() {
    use crate::translate::compose::ObjectRef;
    use crate::translate::effects::ActorRef;
    use byroredux_core::ecs::components::FactionRanks;

    const ACTOR_REF: u32 = 0x000A_1234;
    const FACTION: u32 = 0x0001_38B8;
    const OTHER: u32 = 0x0001_5FFB;
    let mut world = fixture();
    world.register::<FactionRanks>();
    let actor = spawn_with_form_id(&mut world, ACTOR_REF);
    world.insert(actor, FactionRanks::from_pairs([(OTHER, 2)]));
    let player = world.resource::<PapyrusPlayerEntity>().0;

    run_stage_effects(
        &world,
        object_property_vmad(&[("Actor", ACTOR_REF), ("Faction", FACTION), ("Other", OTHER)]),
        vec![
            Effect::ChangeFaction {
                actor: ActorRef::Object(ObjectRef::Property("Actor".into())),
                faction: ObjectRef::Property("Faction".into()),
                change: crate::FactionChange::Add,
            },
            Effect::ChangeFaction {
                actor: ActorRef::Object(ObjectRef::Property("Actor".into())),
                faction: ObjectRef::Property("Other".into()),
                change: crate::FactionChange::Remove,
            },
            Effect::ChangeFaction {
                actor: ActorRef::Player,
                faction: ObjectRef::Property("Faction".into()),
                change: crate::FactionChange::SetRank(3),
            },
        ],
    );
    let ranks = world.get::<FactionRanks>(actor).unwrap();
    assert_eq!(ranks.0, vec![(FACTION, 0)]);
    drop(ranks);
    assert_eq!(world.get::<FactionRanks>(player).unwrap().rank(FACTION), Some(3));
    let ledger = world.resource::<crate::ScriptedFactionChanges>();
    assert_eq!(ledger.changes_for(ACTOR_REF), &[(FACTION, Some(0)), (OTHER, None)]);
    assert_eq!(ledger.changes_for(crate::PLAYER_REFERENCE), &[(FACTION, Some(3))]);
}

/// M47.2 — `RemoveFromAllFactions` empties the live `FactionRanks` and the
/// ledger records every faction the actor was in as removed, so the spawn
/// re-derivation replays the departure.
#[test]
fn dispatch_remove_from_all_factions_records_each_departure() {
    use crate::translate::compose::ObjectRef;
    use crate::translate::effects::ActorRef;
    use byroredux_core::ecs::components::FactionRanks;

    const ACTOR_REF: u32 = 0x000A_1240;
    const A: u32 = 0x0001_38B8;
    const B: u32 = 0x0001_5FFB;
    let mut world = fixture();
    world.register::<FactionRanks>();
    let actor = spawn_with_form_id(&mut world, ACTOR_REF);
    world.insert(actor, FactionRanks::from_pairs([(A, 1), (B, 0)]));

    run_stage_effects(
        &world,
        object_property_vmad(&[("Actor", ACTOR_REF)]),
        vec![Effect::ChangeFaction {
            actor: ActorRef::Object(ObjectRef::Property("Actor".into())),
            faction: ObjectRef::Property(String::new()),
            change: crate::FactionChange::RemoveAll,
        }],
    );
    assert!(world.get::<FactionRanks>(actor).unwrap().is_empty());
    let ledger = world.resource::<crate::ScriptedFactionChanges>();
    assert_eq!(ledger.changes_for(ACTOR_REF), &[(A, None), (B, None)]);
}

/// M47.2 — the IDLEs installed fragments play, resolved through each
/// fragment's VMAD, `If` branches included; what the app decodes HKX for.
#[test]
fn fragment_idle_forms_collects_play_idle_targets() {
    use crate::translate::compose::ObjectRef;
    use crate::translate::effects::ActorRef;

    const WAVE: u32 = 0x0001_0F31;
    const SIT: u32 = 0x0001_0F32;
    let world = fixture();
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert_vmad(Q, object_property_vmad(&[("IdleWave", WAVE), ("IdleSit", SIT)]));
        frags.insert(
            Q,
            10,
            vec![
                Effect::PlayIdle {
                    actor: ActorRef::Player,
                    idle: ObjectRef::Property("IdleWave".into()),
                },
                Effect::Conditional {
                    guards: vec![],
                    then_effects: vec![Effect::PlayIdle {
                        actor: ActorRef::Player,
                        idle: ObjectRef::Property("IdleSit".into()),
                    }],
                    else_effects: vec![Effect::PlayIdle {
                        actor: ActorRef::Player,
                        idle: ObjectRef::Property("Unbound".into()),
                    }],
                },
            ],
        );
    }
    let forms = crate::fragment_idle_forms(&world);
    assert_eq!(forms, [WAVE, SIT].into_iter().collect());
}

/// M47.2 — `SetActorValue` sets the base of the VMAD-named AVIF on the
/// resolved actor.
#[test]
fn dispatch_set_actor_value_sets_the_base() {
    use crate::translate::compose::ObjectRef;
    use crate::translate::effects::ActorRef;
    use byroredux_core::ecs::components::ActorValues;

    const AGGRESSION: u32 = 0x0000_02BB;
    let mut world = fixture();
    world.register::<ActorValues>();
    let player = world.resource::<PapyrusPlayerEntity>().0;
    run_stage_effects(
        &world,
        object_property_vmad(&[("pAggression", AGGRESSION)]),
        vec![Effect::SetActorValue {
            actor: ActorRef::Player,
            actor_value: ObjectRef::Property("pAggression".into()),
            value: crate::translate::effects::ValueExpr::Literal(2.0),
        }],
    );
    assert_eq!(world.get::<ActorValues>(player).unwrap().current(AGGRESSION), 2.0);
}

#[test]
fn dispatch_set_actor_value_routes_player_derived_pool_to_override() {
    use crate::translate::compose::ObjectRef;
    use crate::translate::effects::ActorRef;
    use byroredux_core::character::{
        CharacterLevel, CharacterRuleset, DerivedInput, DerivedStatFormula, LevelingModel,
    };
    use byroredux_core::ecs::components::ActorValues;

    const ENDURANCE: u32 = 0x07;
    const HEALTH: u32 = 0x2D4;
    let mut world = fixture();
    world.register::<ActorValues>();
    let player = world.resource::<PapyrusPlayerEntity>().0;
    let ruleset = CharacterRuleset::new(LevelingModel::FO4).with_derived(
        HEALTH,
        DerivedStatFormula::bilinear(
            DerivedInput::actor_value(ENDURANCE),
            4.5,
            DerivedInput::LEVEL,
            2.5,
            0.5,
            77.5,
        )
        .floored()
        .player_only(),
    );
    let mut player_values = ActorValues::from_pairs([(ENDURANCE, 5.0)]);
    ruleset.refresh_player_only_bases(&mut player_values, 1);
    world.insert(player, player_values);
    world.insert(player, CharacterLevel { level: 1, xp: 0 });
    world.insert_resource(ruleset);

    run_stage_effects(
        &world,
        object_property_vmad(&[("pHealth", HEALTH)]),
        vec![Effect::SetActorValue {
            actor: ActorRef::Player,
            actor_value: ObjectRef::Property("pHealth".into()),
            value: crate::translate::effects::ValueExpr::Literal(500.0),
        }],
    );

    let ruleset = world.resource::<CharacterRuleset>();
    {
        let values = world.get::<ActorValues>(player).unwrap();
        let health = values.get(HEALTH).unwrap();
        assert_eq!(health.base, 105.0, "the ruleset owns the formula base");
        assert_eq!(health.set_override, 500.0);
        assert_eq!(values.current(HEALTH), 605.0);
    }
    let mut refreshed = world.get::<ActorValues>(player).unwrap().clone();
    ruleset.refresh_player_only_bases(&mut refreshed, 1);
    assert_eq!(refreshed.current(HEALTH), 605.0, "the write survives refresh");
    assert_eq!(refreshed.get(HEALTH).unwrap().base, 105.0);
}

/// M47.2 — `ShowBarterMenu` hands the vendor to the engine's barter menu.
#[test]
fn show_barter_menu_requests_the_vendor() {
    const BELETHOR: u32 = 0x0001_3BB6;
    let mut world = fixture();
    let vendor = spawn_with_form_id(&mut world, BELETHOR);
    run_stage_effects(
        &world,
        object_property_vmad(&[("Vendor", BELETHOR)]),
        vec![Effect::ShowBarterMenu {
            vendor: ActorRef::Object(ObjectRef::Property("::Vendor_var".into())),
        }],
    );
    assert_eq!(crate::menus::take_barter_request(&world), Some(vendor));
    assert_eq!(crate::menus::take_barter_request(&world), None, "consumed once");
}

/// M47.2 — an item count computed at run time (`Amount * Reward`).
#[test]
fn item_counts_evaluate_run_time_values() {
    use crate::translate::effects::{ArithOp, ItemCount, ValueExpr};
    use byroredux_core::ecs::components::Inventory;
    let world = fixture();
    let player = world.resource::<crate::papyrus_demo::PapyrusPlayerEntity>().0;
    run_stage_effects(
        &world,
        object_property_vmad(&[("Gold001", 0xF)]),
        vec![Effect::AddItem {
            silent: false,
            container: ActorRef::Player,
            item: ObjectRef::Property("Gold001".into()),
            count: ItemCount::Value(ValueExpr::Arith(
                Box::new(ValueExpr::Literal(3.0)),
                ArithOp::Mul,
                Box::new(ValueExpr::Literal(25.0)),
            )),
        }],
    );
    let gold: u32 = world
        .get::<Inventory>(player)
        .unwrap()
        .items
        .iter()
        .filter(|stack| stack.base_form_id == 0xF)
        .map(|stack| stack.count)
        .sum();
    assert_eq!(gold, 75);
}

/// M47.2 — `AddKeyword` lands on the reference after the fragment, a guard in
/// the same fragment sees it pending, and `RemoveKeyword` takes it back.
#[test]
fn keyword_changes_reach_has_keyword() {
    use crate::keywords::KeywordChange;
    const GUARD: u32 = 0x0002_A1B0;
    const INJURED: u32 = 0x0009_1234;
    let mut world = fixture();
    let guard = spawn_with_form_id(&mut world, GUARD);
    let vmad = || object_property_vmad(&[("Guard", GUARD), ("Injured", INJURED)]);
    let change = |change| Effect::ChangeKeyword {
        object: ActorRef::Object(ObjectRef::Property("::Guard_var".into())),
        keyword: ObjectRef::Property("::Injured_var".into()),
        change,
    };
    run_stage_effects(
        &world,
        vmad(),
        vec![
            change(KeywordChange::Add),
            Effect::Conditional {
                guards: vec![FragmentGuard::HasKeyword {
                    object: ActorRef::Object(ObjectRef::Property("::Guard_var".into())),
                    keyword: ObjectRef::Property("::Injured_var".into()),
                    expected: true,
                }],
                then_effects: vec![Effect::PlayerCaps { delta: 1, silent: false }],
                else_effects: vec![],
            },
        ],
    );
    assert!(crate::keywords::has_keyword(&world, guard, INJURED));
    let player = world.resource::<crate::papyrus_demo::PapyrusPlayerEntity>().0;
    assert_eq!(
        world
            .get::<byroredux_core::ecs::components::Inventory>(player)
            .map_or(0, |inventory| inventory.items.iter().map(|s| s.count).sum::<u32>()),
        1,
        "the guard saw the pending AddKeyword"
    );
    run_stage_effects(&world, vmad(), vec![change(KeywordChange::Remove)]);
    assert!(!crate::keywords::has_keyword(&world, guard, INJURED));
}

/// M47.2 — `SameReference` compares identities by FormID: a direct reference
/// equals itself whether or not it is loaded, and the player is the player.
#[test]
fn same_reference_guards_compare_form_ids() {
    const SWORD_REF: u32 = 0x0004_1E3A;
    // Whether the guard passes: its branch grants one cap.
    let passes = |left: ActorRef, right: Option<ActorRef>, expected: bool| {
        let world = fixture();
        run_stage_effects(
            &world,
            object_property_vmad(&[("Thing", SWORD_REF)]),
            vec![Effect::Conditional {
                guards: vec![FragmentGuard::SameReference {
                    left,
                    right,
                    expected,
                }],
                then_effects: vec![Effect::PlayerCaps { delta: 1, silent: false }],
                else_effects: vec![],
            }],
        );
        let player = world.resource::<crate::papyrus_demo::PapyrusPlayerEntity>().0;
        world
            .get::<byroredux_core::ecs::components::Inventory>(player)
            .is_some_and(|inventory| inventory.items.iter().any(|s| s.base_form_id == 0xF))
    };
    let thing = || ActorRef::Object(ObjectRef::Property("::Thing_var".into()));
    assert!(passes(thing(), Some(thing()), true), "unloaded, still itself");
    assert!(passes(thing(), None, false), "a VMAD reference is not None");
    assert!(!passes(thing(), None, true));
    assert!(passes(ActorRef::Player, Some(thing()), false));
    assert!(passes(ActorRef::Player, Some(ActorRef::Player), true));
}

/// M47.2 — `SetAlly` writes directed Ally / Friend reactions through the same
/// deferred `FactionRelations` path `SetEnemy` takes (CK *SetAlly - Faction*:
/// a `true` flag makes that direction a friend, `false` an ally).
#[test]
fn set_ally_writes_directed_ally_and_friend_reactions() {
    use byroredux_sdk::relationships::CombatReaction;
    const VAMPIRES: u32 = 0x0000_3376;
    const SERANA: u32 = 0x0000_3377;
    let mut world = fixture();
    let identity = crate::LoadOrderIdentity::new(vec![(
        byroredux_plugin::esm::reader::GlobalSlot::Regular(0),
        byroredux_core::form_id::PluginId::from_filename("Skyrim.esm"),
    )]);
    let vampires = identity.form_ref(VAMPIRES).unwrap();
    let serana = identity.form_ref(SERANA).unwrap();
    world.insert_resource(identity);
    run_stage_effects(
        &world,
        object_property_vmad(&[("Vampires", VAMPIRES), ("Serana", SERANA)]),
        vec![Effect::SetAlly {
            faction: ObjectRef::Property("::Vampires_var".into()),
            other_faction: ObjectRef::Property("::Serana_var".into()),
            self_friend: false,
            other_friend: true,
        }],
    );
    let relations = world.resource::<crate::FactionRelations>();
    assert_eq!(relations.reaction(vampires, serana), Some(CombatReaction::Ally));
    assert_eq!(relations.reaction(serana, vampires), Some(CombatReaction::Friend));
}

/// M47.2 — a quest scope's bindings carry the fragment's property into the
/// quest's VMAD under the parameter's name: `MoveSpouse(…, SolitudeHouse)`
/// from an INFO enables the INFO's marker through the quest-scoped body.
#[test]
fn quest_scope_bindings_carry_the_fragments_properties() {
    use crate::translate::compose::QuestRef;
    const HOUSE_MARKER: u32 = 0x000E_F21E;
    let world = fixture();
    let mut instances = HashMap::new();
    // The quest's own VMAD knows nothing of the house.
    instances.insert(Q, object_property_vmad(&[]));
    let mut world = world;
    world.insert_resource(super::QuestScriptInstances(std::sync::Arc::new(instances)));
    world
        .resource_mut::<crate::fragment::ReferenceEnableState>()
        .set_enabled(HOUSE_MARKER, false);
    run_stage_effects(
        &world,
        object_property_vmad(&[("SolitudeHouse", HOUSE_MARKER)]),
        vec![Effect::QuestScope {
            quest: QuestRef::OwningQuest,
            effects: vec![Effect::Enable {
                object: ObjectRef::Property("inline.spousescript.movespouse.newhouse".into()),
                fade_in: false,
            }],
            bindings: vec![(
                "inline.spousescript.movespouse.newhouse".into(),
                "SolitudeHouse".into(),
            )],
        }],
    );
    assert!(super::reference_enabled(&world, HOUSE_MARKER));
}

/// M47.2 — a linked ref resolves through its loaded source's `XLKR` links,
/// under the keyword or the unkeyworded link, and a FormID-keyed effect
/// reaches a target that is not loaded.
#[test]
fn linked_refs_resolve_through_the_sources_links() {
    const SOURCE: u32 = 0x0001_0A00;
    const LINK_GATE: u32 = 0x0009_0001;
    const GATE: u32 = 0x0001_0A01;
    const LEVER: u32 = 0x0001_0A02;
    let mut world = fixture();
    let source = spawn_with_form_id(&mut world, SOURCE);
    world.register::<crate::scene::SceneAliasCandidate>();
    world.insert(
        source,
        crate::scene::SceneAliasCandidate {
            reference_form_id: SOURCE,
            linked_refs: vec![(LINK_GATE, GATE), (0, LEVER)],
            ..Default::default()
        },
    );
    let linked = |keyword: Option<&str>| ObjectRef::Linked {
        from: Box::new(ObjectRef::Property("Source".into())),
        keyword: keyword.map(|k| Box::new(ObjectRef::Property(k.into()))),
    };
    run_stage_effects(
        &world,
        object_property_vmad(&[("Source", SOURCE), ("LinkGate", LINK_GATE)]),
        vec![
            Effect::Disable {
                object: linked(Some("LinkGate")),
                fade_out: false,
            },
            Effect::Disable {
                object: linked(None),
                fade_out: false,
            },
        ],
    );
    assert!(!super::reference_enabled(&world, GATE), "keyworded link");
    assert!(!super::reference_enabled(&world, LEVER), "unkeyworded link");
}

/// `MoveToMyEditorLocation` puts a moved reference back at its authored pose.
#[test]
fn move_to_editor_location_restores_the_authored_pose() {
    use byroredux_core::ecs::Transform;
    use byroredux_core::math::{Quat, Vec3};
    const MOVED: u32 = 0x0001_0A20;
    let mut world = fixture();
    let entity = spawn_with_form_id(&mut world, MOVED);
    world.register::<crate::EditorLocation>();
    world.register::<Transform>();
    let authored = crate::EditorLocation {
        translation: Vec3::new(1.0, 2.0, 3.0),
        rotation: Quat::from_rotation_y(0.5),
    };
    world.insert(entity, authored);
    world.insert(entity, Transform::new(Vec3::new(9.0, 9.0, 9.0), Quat::IDENTITY, 1.0));
    run_stage_effects(
        &world,
        object_property_vmad(&[("Moved", MOVED)]),
        vec![Effect::MoveToEditorLocation {
            moved: ObjectRef::Property("Moved".into()),
        }],
    );
    let transform = *world.get::<Transform>(entity).unwrap();
    assert_eq!(transform.translation, authored.translation);
    assert_eq!(transform.rotation, authored.rotation);
}

/// `SetLinkedRef` overrides the authored link under its keyword; `None`
/// clears it; links under another keyword are untouched.
#[test]
fn set_linked_ref_overrides_and_clears_links() {
    const SOURCE: u32 = 0x0001_0A10;
    const LINK_GATE: u32 = 0x0009_0001;
    const AUTHORED: u32 = 0x0001_0A11;
    const REPLACED: u32 = 0x0001_0A12;
    const LEVER: u32 = 0x0001_0A13;
    let mut world = fixture();
    let source = spawn_with_form_id(&mut world, SOURCE);
    world.register::<crate::scene::SceneAliasCandidate>();
    world.insert(
        source,
        crate::scene::SceneAliasCandidate {
            reference_form_id: SOURCE,
            linked_refs: vec![(LINK_GATE, AUTHORED), (0, LEVER)],
            ..Default::default()
        },
    );
    world.init_resource::<crate::LinkedRefOverrides>();
    assert_eq!(crate::linked_ref(&world, SOURCE, LINK_GATE), Some(AUTHORED));
    let vmad = object_property_vmad(&[
        ("Source", SOURCE),
        ("LinkGate", LINK_GATE),
        ("Replaced", REPLACED),
    ]);
    run_stage_effects(
        &world,
        vmad.clone(),
        vec![Effect::SetLinkedRef {
            object: ObjectRef::Property("Source".into()),
            target: Some(ObjectRef::Property("Replaced".into())),
            keyword: Some(ObjectRef::Property("LinkGate".into())),
        }],
    );
    assert_eq!(crate::linked_ref(&world, SOURCE, LINK_GATE), Some(REPLACED));
    assert_eq!(crate::linked_ref(&world, SOURCE, 0), Some(LEVER), "other keyword untouched");
    run_stage_effects(
        &world,
        vmad,
        vec![Effect::SetLinkedRef {
            object: ObjectRef::Property("Source".into()),
            target: None,
            keyword: Some(ObjectRef::Property("LinkGate".into())),
        }],
    );
    assert_eq!(crate::linked_ref(&world, SOURCE, LINK_GATE), None, "cleared");
}


/// M47.2 — the CK *Quest Stages Tab*: a stage runs on start only when it is
/// flagged "Start Up Stage". A quest started without one (Skyrim's MS12, Start
/// Game Enabled with no flagged stage) must not run its stage 0 fragment,
/// which here — as in MS12 — would move the player.
#[test]
fn a_quest_started_without_a_start_up_stage_runs_no_stage_fragment() {
    const STARTED: QuestFormId = QuestFormId(0x0002_C258);
    const FLAGGED: QuestFormId = QuestFormId(0x0004_B2D9);
    let world = fixture();
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        for quest in [STARTED, FLAGGED] {
            frags.insert(quest, 0, vec![Effect::PlayerCaps { delta: 1, silent: false }]);
        }
    }
    let caps = |world: &World| {
        let player = world.resource::<PapyrusPlayerEntity>().0;
        world
            .get::<byroredux_core::ecs::components::Inventory>(player)
            .map_or(0, |inventory| {
                inventory.items.iter().filter(|s| s.base_form_id == 0xF).map(|s| s.count).sum::<u32>()
            })
    };
    world.resource_mut::<QuestStageState>().start_quest(STARTED, None);
    quest_fragment_dispatch_system(&world);
    assert_eq!(caps(&world), 0, "no Start Up Stage: stage 0 is not set");
    world.resource_mut::<QuestStageState>().start_quest(FLAGGED, Some(0));
    quest_fragment_dispatch_system(&world);
    assert_eq!(caps(&world), 1, "a flagged Start Up Stage 0 runs");
}

/// M47.2 — `WaitWhile` polls: suspended while its guard holds (re-checked at
/// each resume), the rest of the fragment runs once it fails.
#[test]
fn wait_while_polls_until_its_condition_fails() {
    use crate::translate::effects::GuardComparison;
    let world = fixture();
    world.resource_mut::<QuestStageFragments>().insert(
        Q,
        1,
        vec![
            Effect::WaitWhile {
                guards: vec![FragmentGuard::Stage {
                    quest: QuestRef::SelfRef,
                    comparison: GuardComparison::Lt,
                    value: 10.0,
                }],
                poll_seconds: 1.0,
            },
            Effect::PlayerCaps { delta: 1, silent: false },
        ],
    );
    let caps = |world: &World| {
        let player = world.resource::<PapyrusPlayerEntity>().0;
        world
            .get::<byroredux_core::ecs::components::Inventory>(player)
            .map_or(0, |inventory| {
                inventory.items.iter().filter(|s| s.base_form_id == 0xF).map(|s| s.count).sum::<u32>()
            })
    };
    world.resource_mut::<QuestStageState>().set_stage(Q, 1);
    emit_advance(&world, Q, 1);
    quest_fragment_dispatch_system(&world);
    assert_eq!(caps(&world), 0, "waiting");
    fragment_continuation_system(&world, 1.0);
    assert_eq!(caps(&world), 0, "still stage 1 < 10: waiting again");
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    fragment_continuation_system(&world, 1.0);
    assert_eq!(caps(&world), 1, "the condition failed: the fragment ran on");
}

/// M47.2 — a wait inside a conditional branch suspends the fragment and the
/// resume runs the rest of the branch, then the rest of the program.
#[test]
fn a_wait_inside_a_branch_resumes_into_the_branch_then_the_program() {
    let world = fixture();
    world.resource_mut::<QuestStageFragments>().insert(
        Q,
        1,
        vec![
            Effect::Conditional {
                guards: vec![FragmentGuard::StageDone {
                    quest: QuestRef::SelfRef,
                    stage: 1,
                    done: true,
                }],
                then_effects: vec![Effect::Wait { seconds: 1.0 }, Effect::PlayerCaps { delta: 1, silent: false }],
                else_effects: vec![],
            },
            Effect::PlayerCaps { delta: 10, silent: false },
        ],
    );
    let caps = |world: &World| {
        let player = world.resource::<PapyrusPlayerEntity>().0;
        world
            .get::<byroredux_core::ecs::components::Inventory>(player)
            .map_or(0, |inventory| {
                inventory.items.iter().filter(|s| s.base_form_id == 0xF).map(|s| s.count).sum::<u32>()
            })
    };
    world.resource_mut::<QuestStageState>().set_stage(Q, 1);
    emit_advance(&world, Q, 1);
    quest_fragment_dispatch_system(&world);
    assert_eq!(caps(&world), 0, "suspended inside the branch");
    fragment_continuation_system(&world, 1.0);
    assert_eq!(caps(&world), 11, "the branch's rest, then the program's");
}

/// `Activate(…, true)` is queued apart and flushed as an `ActivateEvent` carrying
/// the `NativeActivation` flag; a plain activation carries none.
#[test]
fn native_activations_flush_with_their_flag() {
    let mut world = World::new();
    crate::register(&mut world);
    world.insert_resource(crate::PendingFragmentActivations::default());
    let (plain, native, activator) = (world.spawn(), world.spawn(), world.spawn());
    {
        let mut queue = world.resource_mut::<crate::PendingFragmentActivations>();
        queue.push(plain, activator);
        queue.push_native(native, activator);
        assert_eq!(queue.len(), 2);
    }
    crate::fragment_activation_flush_system(&world, 0.016);
    assert!(world.get::<crate::ActivateEvent>(plain).is_some());
    assert!(world.get::<crate::NativeActivation>(plain).is_none());
    assert_eq!(world.get::<crate::ActivateEvent>(native).unwrap().activator, activator);
    assert!(world.get::<crate::NativeActivation>(native).is_some());
    assert!(world.resource::<crate::PendingFragmentActivations>().is_empty());
}

/// #5887 — a `.psc`-sourced remote handler (`Event Actor.OnDeath(…)`) is
/// spelled `Actor_OnDeath` like its `.pex` twin, so the VM's matcher binds
/// it; the script's own `OnDeath` lookup still declines it.
#[test]
fn psc_remote_event_handler_binds_like_the_pex_spelling() {
    let (script, errs) = byroredux_papyrus::parse_script(
        "ScriptName Foo\n\
         Event Actor.OnDeath(Actor akSender, Actor akKiller)\n\
         EndEvent\n",
    )
    .expect("parses");
    assert!(errs.is_empty(), "{errs:?}");
    assert_eq!(
        super::events::handler_in(&script, "OnDeath", true).as_deref(),
        Some("Actor_OnDeath")
    );
    assert_eq!(super::events::handler_in(&script, "OnDeath", false), None);
}

/// #5871 — repeated scripted adds and barter-style transfers fold into one
/// row per form on both sides instead of appending a row per transfer.
#[test]
fn repeated_add_and_transfer_do_not_fragment_inventory_rows() {
    use byroredux_core::ecs::components::{Inventory, ItemStack};
    const GOLD: u32 = 0xF;
    let mut world = World::new();
    world.register::<Inventory>();
    let (player, vendor) = (world.spawn(), world.spawn());
    world.insert(vendor, Inventory { items: vec![ItemStack::new(GOLD, 100), ItemStack::new(0x77, 3)] });
    crate::add_items(&world, player, GOLD, 10);
    crate::add_items(&world, player, GOLD, 5);
    for _ in 0..3 {
        crate::remove_items(&world, vendor, 0x77, 1, Some(player));
        crate::remove_items(&world, player, GOLD, 2, Some(vendor));
    }
    let rows = |e| world.get::<Inventory>(e).unwrap().items.clone();
    assert_eq!(rows(player), vec![ItemStack::new(GOLD, 9), ItemStack::new(0x77, 3)]);
    assert_eq!(rows(vendor), vec![ItemStack::new(GOLD, 106), ItemStack::new(0x77, 0)]);
}

/// #5863 — taking an equipped item out of an inventory (a barter sale, a
/// scripted `RemoveItem`) is an unequip: the slot is released, the combat
/// weapon goes with its row, and the unequip is announced.
#[test]
fn removing_an_equipped_item_unequips_it() {
    use byroredux_core::ecs::components::{
        EquipmentSlots, EquippedWeapon, Inventory, InventoryIndex, ItemStack,
    };
    const SWORD: u32 = 0x1234;
    const SHIRT: u32 = 0x5678;
    let mut world = World::new();
    world.register::<Inventory>();
    world.register::<EquipmentSlots>();
    world.register::<EquippedWeapon>();
    world.register::<crate::EquipmentEventBatch>();
    world.register::<crate::ItemEventBatch>();
    let (actor, vendor) = (world.spawn(), world.spawn());
    world.insert(
        actor,
        Inventory { items: vec![ItemStack::new(SWORD, 1), ItemStack::new(SHIRT, 1)] },
    );
    let mut slots = EquipmentSlots::new();
    slots.equip_weapon(InventoryIndex(0));
    slots.equip(0b10, InventoryIndex(1));
    world.insert(actor, slots);
    world.insert(
        actor,
        EquippedWeapon {
            inventory_index: InventoryIndex(0),
            base_form_id: SWORD,
            damage: 12.0,
            reach: 1.0,
            speed: 1.0,
        },
    );
    crate::remove_items(&world, actor, SWORD, 1, Some(vendor));
    assert!(world.get::<EquippedWeapon>(actor).is_none(), "the sold weapon stops hitting");
    assert_eq!(world.get::<EquipmentSlots>(actor).unwrap().weapon, None);
    assert!(world.get::<EquipmentSlots>(actor).unwrap().is_equipped(InventoryIndex(1)), "the shirt stays on");
    assert_eq!(
        world.get::<crate::EquipmentEventBatch>(actor).unwrap().0,
        vec![crate::EquipmentChange { item_form_id: SWORD, equipped: false }],
    );

    // A partial take from a still-held stack changes nothing.
    world.get_mut::<Inventory>(actor).unwrap().items[1].count = 3;
    crate::remove_items(&world, actor, SHIRT, 1, None);
    assert!(world.get::<EquipmentSlots>(actor).unwrap().is_equipped(InventoryIndex(1)));
    assert_eq!(world.get::<crate::EquipmentEventBatch>(actor).unwrap().0.len(), 1);
}
