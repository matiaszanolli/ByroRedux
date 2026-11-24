use super::*;
use crate::papyrus_demo::PapyrusPlayerEntity;
use crate::translate::effects::Effect;
use byroredux_core::ecs::components::FormIdComponent;
use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

const Q: QuestFormId = QuestFormId(0x0001_0F00);
const GUARD: u32 = 0x0001_0F01;
const MARKER: u32 = 0x0001_0F02;

const SOURCE: &str = "ScriptName QF_VmTest extends Quest\n\
    ReferenceAlias Property Alias_Guard Auto\n\
    ObjectReference Property Marker Auto\n\
    Function Fragment_1()\n\
      ObjectReference guard = Alias_Guard.GetReference()\n\
      If guard\n\
        guard.Disable()\n\
      EndIf\n\
      Int count = 0\n\
      While count < 3\n\
        count += 1\n\
      EndWhile\n\
      If count == Self.GetStage() - 7\n\
        Self.SetStage(20)\n\
      EndIf\n\
      Marker.Enable()\n\
    EndFunction\n";

fn parsed() -> Arc<Script> {
    let (script, errors) = byroredux_papyrus::parse_script(SOURCE).expect("parses");
    assert!(errors.is_empty(), "{errors:?}");
    Arc::new(script)
}

fn vmad() -> ScriptInstanceData {
    ScriptInstanceData {
        version: 5,
        object_format: 2,
        scripts: vec![ScriptInstance {
            name: "QF_VmTest".into(),
            status: 0,
            properties: vec![
                ScriptProperty {
                    name: "Alias_Guard".into(),
                    status: 1,
                    value: PropertyValue::Object {
                        form_id: Q.0,
                        alias: 0,
                    },
                },
                ScriptProperty {
                    name: "Marker".into(),
                    status: 1,
                    value: PropertyValue::Object {
                        form_id: MARKER,
                        alias: -1,
                    },
                },
            ],
        }],
    }
}

/// The lowering declines this fragment (an object local from a call, a
/// counting loop, a computed comparison); the VM's check takes it.
#[test]
fn the_check_takes_what_the_lowering_declines() {
    let script = parsed();
    let body = callable_in(&script, None, "Fragment_1").unwrap().body.to_vec();
    assert_eq!(
        crate::translate::effects::lower_fragment(&body),
        None,
        "the lowering declines it"
    );
    let scripts = check_function(Arc::clone(&script), "Fragment_1", None).expect("the VM runs it");
    assert_eq!(scripts.len(), 1);
    // A function called as a statement may wait; one called for its value
    // may not.
    let (waits, _) = byroredux_papyrus::parse_script(
        "ScriptName QF_Wait extends Quest\nFunction Fragment_1()\n Utility.Wait(1.0)\n Helper()\n EndFunction\n\
         Function Fragment_2()\n Int n = Helper()\n EndFunction\n\
         Int Function Helper()\n Utility.Wait(1.0)\n Return 1\n EndFunction\n",
    )
    .unwrap();
    let waits = Arc::new(waits);
    assert!(check_function(Arc::clone(&waits), "Fragment_1", None).is_some());
    assert_eq!(
        check_function_reason(waits, "Fragment_2", None).err().as_deref(),
        Some("a wait in a function called for its value")
    );
}

/// Installed as `Effect::Interpret`, the fragment runs through the stage
/// dispatcher: the alias's reference is disabled, the loop counts to 3, the
/// computed comparison against `GetStage()` (10 - 7) holds and sets stage 20,
/// and the marker is enabled.
#[test]
fn an_interpreted_fragment_runs_through_stage_dispatch() {
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(GUARD),
    });
    world.insert_resource(pool);
    let guard = world.spawn();
    world.insert(guard, FormIdComponent(id));
    world
        .resource_mut::<crate::SceneActorBindings>()
        .bind(Q, 0, guard);
    world
        .resource_mut::<crate::fragment::ReferenceEnableState>()
        .set_enabled(MARKER, false);
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        fragments.insert_vmad(Q, vmad());
        fragments.insert_vm_script(parsed());
        fragments.insert(
            Q,
            10,
            vec![Effect::Interpret {
                script: "QF_VmTest".into(),
                function: "Fragment_1".into(),
                args: Vec::new(),
                resume: None,
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert!(!crate::fragment::reference_enabled(&world, GUARD), "the alias's reference");
    assert!(crate::fragment::reference_enabled(&world, MARKER));
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// An object script variable holds its reference from one fragment to the
/// next (`QuestScriptObjects`), as Papyrus script state does.
#[test]
fn object_script_variables_outlive_the_fragment_that_wrote_them() {
    const SOURCE: &str = "ScriptName QF_VmObjects extends Quest\n\
        ReferenceAlias Property Alias_Guard Auto\n\
        ObjectReference Stored\n\
        Function Fragment_1()\n Stored = Alias_Guard.GetReference()\n EndFunction\n\
        Function Fragment_2()\n Stored.Disable()\n EndFunction\n";
    let (script, errors) = byroredux_papyrus::parse_script(SOURCE).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let script = Arc::new(script);
    for fragment in ["Fragment_1", "Fragment_2"] {
        assert!(check_function(Arc::clone(&script), fragment, None).is_some(), "{fragment}");
    }
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    world.insert_resource(crate::QuestScriptObjects::default());
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(GUARD),
    });
    world.insert_resource(pool);
    let guard = world.spawn();
    world.insert(guard, FormIdComponent(id));
    world.resource_mut::<crate::SceneActorBindings>().bind(Q, 0, guard);
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        fragments.insert_vmad(Q, vmad());
        fragments.insert_vm_script(Arc::clone(&script));
        for (stage, function) in [(10, "Fragment_1"), (20, "Fragment_2")] {
            fragments.insert(
                Q,
                stage,
                vec![Effect::Interpret {
                    script: "QF_VmObjects".into(),
                    function: function.into(),
                    args: Vec::new(),
                resume: None,
                }],
            );
        }
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    // The alias is cleared before the second fragment: the variable keeps it.
    *world.resource_mut::<crate::SceneActorBindings>() = Default::default();
    world.resource_mut::<QuestStageState>().set_stage(Q, 20);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert!(!crate::fragment::reference_enabled(&world, GUARD));
}

/// `Utility.Wait` suspends the run; each resume continues at the wait — here
/// one at the top level, then one inside an `If` branch — with the locals it
/// had.
#[test]
fn waits_suspend_and_resume_where_they_were() {
    const SOURCE: &str = "ScriptName QF_VmWaits extends Quest\n\
        ObjectReference Property Marker Auto\n\
        Function Fragment_1()\n\
          Int n = 1\n\
          Utility.Wait(1.0)\n\
          If n == 1\n\
            Marker.Enable()\n\
            Utility.Wait(2.0)\n\
            Self.SetStage(20)\n\
          EndIf\n\
        EndFunction\n";
    let (script, errors) = byroredux_papyrus::parse_script(SOURCE).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let script = Arc::new(script);
    assert!(check_function(Arc::clone(&script), "Fragment_1", None).is_some());
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    world
        .resource_mut::<crate::fragment::ReferenceEnableState>()
        .set_enabled(MARKER, false);
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        fragments.insert_vmad(Q, vmad());
        fragments.insert_vm_script(Arc::clone(&script));
        fragments.insert(
            Q,
            10,
            vec![Effect::Interpret {
                script: "QF_VmWaits".into(),
                function: "Fragment_1".into(),
                args: Vec::new(),
                resume: None,
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert!(!crate::fragment::reference_enabled(&world, MARKER), "waiting 1 s");
    crate::fragment::fragment_continuation_system(&world, 1.0);
    assert!(crate::fragment::reference_enabled(&world, MARKER), "resumed into the If");
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 10, "waiting 2 s");
    crate::fragment::fragment_continuation_system(&world, 2.0);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// A wait inside a called function suspends the whole call chain; the resume
/// finishes the callee — its locals intact — and then the caller after the
/// call.
#[test]
fn a_wait_in_a_called_function_resumes_the_call_chain() {
    const SOURCE: &str = "ScriptName QF_VmCallWaits extends Quest\n\
        ObjectReference Property Marker Auto\n\
        Function Fragment_1()\n\
          Helper(2)\n\
          Self.SetStage(30)\n\
        EndFunction\n\
        Function Helper(Int target)\n\
          Utility.Wait(1.0)\n\
          Marker.Enable()\n\
          If target == 2\n\
            Utility.Wait(1.0)\n\
            Self.SetStage(20)\n\
          EndIf\n\
        EndFunction\n";
    let (script, errors) = byroredux_papyrus::parse_script(SOURCE).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let script = Arc::new(script);
    assert!(check_function(Arc::clone(&script), "Fragment_1", None).is_some());
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    world
        .resource_mut::<crate::fragment::ReferenceEnableState>()
        .set_enabled(MARKER, false);
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        fragments.insert_vmad(Q, vmad());
        fragments.insert_vm_script(Arc::clone(&script));
        fragments.insert(
            Q,
            10,
            vec![Effect::Interpret {
                script: "QF_VmCallWaits".into(),
                function: "Fragment_1".into(),
                args: Vec::new(),
                resume: None,
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert!(!crate::fragment::reference_enabled(&world, MARKER), "waiting in Helper");
    crate::fragment::fragment_continuation_system(&world, 1.0);
    assert!(crate::fragment::reference_enabled(&world, MARKER), "Helper resumed");
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 10, "waiting in Helper's If");
    crate::fragment::fragment_continuation_system(&world, 1.0);
    // Helper sets 20 with the parameter it was called with, then the caller
    // continues after the call and sets 30.
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 30);
}

/// #5681 — a `Utility.Wait` past the shared cap stops the run instead of
/// parking an immortal continuation. The literal arm is the value the
/// lowerer already declines; the computed arm is a run-time number only
/// the VM sees. Both die with nothing queued, and the code after the wait
/// never runs.
#[test]
fn waits_past_the_shared_cap_stop_the_run() {
    for (name, source) in [
        (
            "QF_VmWaitCapLiteral",
            "ScriptName QF_VmWaitCapLiteral extends Quest\n\
             Function Fragment_1()\n\
               Utility.Wait(86401.0)\n\
               Self.SetStage(20)\n\
             EndFunction\n",
        ),
        (
            "QF_VmWaitCapComputed",
            "ScriptName QF_VmWaitCapComputed extends Quest\n\
             Function Fragment_1()\n\
               Float secs = 864010.0\n\
               Utility.Wait(secs)\n\
               Self.SetStage(20)\n\
             EndFunction\n",
        ),
    ] {
        let (script, errors) = byroredux_papyrus::parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let script = Arc::new(script);
        assert!(
            check_function(Arc::clone(&script), "Fragment_1", None).is_some(),
            "the check still takes the handler; the cap is a run-time decline"
        );
        let mut world = World::new();
        crate::register(&mut world);
        let player = world.spawn();
        world.insert_resource(PapyrusPlayerEntity(player));
        world.insert_resource(QuestStageState::default());
        {
            let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
            fragments.insert_vmad(Q, vmad());
            fragments.insert_vm_script(Arc::clone(&script));
            fragments.insert(
                Q,
                10,
                vec![Effect::Interpret {
                    script: name.into(),
                    function: "Fragment_1".into(),
                    args: Vec::new(),
                    resume: None,
                }],
            );
        }
        world.resource_mut::<QuestStageState>().set_stage(Q, 10);
        crate::fragment::quest_fragment_dispatch_system(&world);
        assert_eq!(
            world.resource::<QuestStageState>().get_stage(Q),
            10,
            "{name}: the wait must stop the run, not run the code behind it"
        );
        assert!(
            world.resource::<crate::FragmentExecutionQueue>().is_empty(),
            "{name}: an over-cap wait must not park a continuation"
        );
        // No amount of time satisfies it: the queue stays empty forever.
        crate::fragment::fragment_continuation_system(&world, 1.0e30);
        assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 10);
    }
}

/// #6216 — Int-declared natives read as Int and typed locals keep their type, so
/// integer division truncates as in Papyrus: `GetValueInt()` of 12.5 is 12,
/// `12 / 5` is 2 (not 2.4), `GetStage() / 4` at stage 10 is 2, `Float f = 3`
/// divides as a float, and `Int j = 7` assigned 7.9 holds 7.
#[test]
fn int_natives_and_typed_locals_divide_as_papyrus_does() {
    const CLOCK: u32 = 0x0001_0F10;
    const OUT: [u32; 3] = [0x0001_0F11, 0x0001_0F12, 0x0001_0F13];
    const SOURCE: &str = "ScriptName QF_VmInts extends Quest\n\
        GlobalVariable Property Clock Auto\n\
        GlobalVariable Property OutA Auto\n\
        GlobalVariable Property OutB Auto\n\
        GlobalVariable Property OutC Auto\n\
        Function Fragment_1()\n\
          Int i = Clock.GetValueInt()\n\
          OutA.SetValue(i / 5)\n\
          OutB.SetValue(Self.GetStage() / 4)\n\
          Float f = 3\n\
          Int j = 7\n\
          j = 7.9\n\
          OutC.SetValue(f / 2 + j)\n\
        EndFunction\n";
    let (script, errors) = byroredux_papyrus::parse_script(SOURCE).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let script = Arc::new(script);
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    let mut globals = crate::Globals::default();
    globals.set(CLOCK, 12.5);
    for out in OUT {
        globals.set(out, -1.0);
    }
    world.insert_resource(globals);
    let property = |name: &str, form_id: u32| ScriptProperty {
        name: name.into(),
        status: 1,
        value: PropertyValue::Object { form_id, alias: -1 },
    };
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        fragments.insert_vmad(
            Q,
            ScriptInstanceData {
                version: 5,
                object_format: 2,
                scripts: vec![ScriptInstance {
                    name: "QF_VmInts".into(),
                    status: 0,
                    properties: vec![
                        property("Clock", CLOCK),
                        property("OutA", OUT[0]),
                        property("OutB", OUT[1]),
                        property("OutC", OUT[2]),
                    ],
                }],
            },
        );
        fragments.insert_vm_script(Arc::clone(&script));
        fragments.insert(Q, 10, vec![Effect::interpret("QF_VmInts", "Fragment_1", Vec::new())]);
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    let globals = world.resource::<crate::Globals>();
    assert_eq!(globals.get(OUT[0]), Some(2.0), "GetValueInt truncates, then Int / Int");
    assert_eq!(globals.get(OUT[1]), Some(2.0), "GetStage is an Int");
    assert_eq!(globals.get(OUT[2]), Some(8.5), "Float f / 2 + Int j(7)");
}

/// `Global.Value = n` is `Global.SetValue(n)`; with `+=` it counts from the
/// global's current value.
#[test]
fn global_value_assignment_writes_the_global() {
    const COUNT: u32 = 0x0001_0F03;
    const SOURCE: &str = "ScriptName QF_VmGlobals extends Quest\n\
        GlobalVariable Property Count Auto\n\
        Function Fragment_1()\n\
          Count.Value += 2\n\
          Count.Value = Count.Value * 10\n\
        EndFunction\n";
    let (script, errors) = byroredux_papyrus::parse_script(SOURCE).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let script = Arc::new(script);
    assert_eq!(
        check_function_reason(Arc::clone(&script), "Fragment_1", None).err(),
        None
    );
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    let mut globals = crate::Globals::default();
    globals.set(COUNT, 1.0);
    world.insert_resource(globals);
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        fragments.insert_vmad(
            Q,
            ScriptInstanceData {
                version: 5,
                object_format: 2,
                scripts: vec![ScriptInstance {
                    name: "QF_VmGlobals".into(),
                    status: 0,
                    properties: vec![ScriptProperty {
                        name: "Count".into(),
                        status: 1,
                        value: PropertyValue::Object {
                            form_id: COUNT,
                            alias: -1,
                        },
                    }],
                }],
            },
        );
        fragments.insert_vm_script(Arc::clone(&script));
        fragments.insert(
            Q,
            10,
            vec![Effect::Interpret {
                script: "QF_VmGlobals".into(),
                function: "Fragment_1".into(),
                args: Vec::new(),
                resume: None,
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<crate::Globals>().get(COUNT), Some(30.0));
}

/// A one-quest world with `script` installed for `Q`'s stage-10 fragment
/// `Fragment_1`, `MARKER` disabled.
fn world_with_fragment(script: Arc<Script>) -> World {
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    world.insert_resource(crate::QuestScriptObjects::default());
    world
        .resource_mut::<crate::fragment::ReferenceEnableState>()
        .set_enabled(MARKER, false);
    let name = script.name.node.0.clone();
    let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
    fragments.insert_vmad(Q, vmad());
    fragments.insert_vm_script(script);
    fragments.insert(
        Q,
        10,
        vec![Effect::Interpret {
            script: name,
            function: "Fragment_1".into(),
            args: Vec::new(),
            resume: None,
        }],
    );
    drop(fragments);
    world
}

fn parsed_source(source: &str) -> Arc<Script> {
    let (script, errors) = byroredux_papyrus::parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    Arc::new(script)
}

/// A call runs the version of the current state (the auto state at first);
/// `GotoState` switches it and runs the new state's `OnBeginState`.
#[test]
fn calls_run_the_version_of_the_current_state() {
    let script = parsed_source(
        "ScriptName QF_VmStates extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n Ping()\n Ping()\n EndFunction\n\
         Function Ping()\n EndFunction\n\
         Auto State Waiting\n\
           Function Ping()\n Self.SetStage(20)\n GotoState(\"Done\")\n EndFunction\n\
         EndState\n\
         State Done\n\
           Event OnBeginState()\n Marker.Enable()\n EndEvent\n\
           Function Ping()\n If GetState() == \"Done\"\n Self.SetStage(30)\n EndIf\n EndFunction\n\
         EndState\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let world = world_with_fragment(script);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert!(crate::fragment::reference_enabled(&world, MARKER), "Done's OnBeginState");
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 30, "the second Ping ran Done's version");
}

/// An unwritten script variable or property reads what the script declares;
/// a `String` variable holds what a script writes to it.
#[test]
fn script_variables_read_their_declared_values_and_hold_strings() {
    let script = parsed_source(
        "ScriptName QF_VmDeclared extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Int Property Goal = 3 Auto\n\
         String Label = \"start\"\n\
         Function Fragment_1()\n\
           If Goal == 3 && Label == \"start\"\n Label = \"next\"\n EndIf\n\
           If Label == \"next\"\n Marker.Enable()\n EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let world = world_with_fragment(script);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert!(crate::fragment::reference_enabled(&world, MARKER));
}

/// #5443 — the `.pex` decompiler now lowers opcode 36 to `Expr::Is`; the VM
/// evaluates the type-test as the operand's truthiness (form types are not
/// tracked), which is what the object-typed `Cast` it replaced gave.
#[test]
fn an_is_type_test_runs_as_the_operands_truthiness() {
    let script = parsed_source(
        "ScriptName QF_VmIs extends Quest\n\
         ObjectReference Property Marker Auto\n\
         ObjectReference Property Nothing Auto\n\
         Function Fragment_1()\n\
           If Nothing is Actor\n Self.SetStage(99)\n EndIf\n\
           If Marker is ObjectReference\n Marker.Enable()\n EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let world = world_with_fragment(script);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert!(crate::fragment::reference_enabled(&world, MARKER), "`Marker is …` is true");
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 10, "`None is Actor` is false");
}

/// Events the engine raises outside a script run reach the reference's handler
/// with their arguments: a trap hit with its ten, a destruction stage change with
/// both stage numbers.
#[test]
fn raised_reference_events_reach_the_handlers_with_their_arguments() {
    use crate::translate::effects::VmStored;
    const REF: u32 = 0x0001_0F11;
    const TARGET: u32 = 0x0001_0F12;
    let script = parsed_source(
        "ScriptName VmRaisedScript extends ObjectReference\n\
         Int Property Hits Auto\n\
         Int Property Motion Auto\n\
         Int Property Stage Auto\n\
         Int Property Stopped Auto\n\
         Event OnTrapHitStart(ObjectReference akTarget, Float afXVel, Float afYVel, Float afZVel, Float afXPos, Float afYPos, Float afZPos, Int aeMaterial, Bool abInitialHit, Int aeMotionType)\n\
           If abInitialHit && afZPos > 2.5\n\
             Hits += 1\n\
             Motion = aeMotionType\n\
           EndIf\n\
         EndEvent\n\
         Event OnTrapHitStop(ObjectReference akTarget)\n Stopped += 1\n EndEvent\n\
         Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)\n Stage = aiCurrentStage\n EndEvent\n",
    );
    let (events, scripts) =
        crate::check_reference_script(Arc::clone(&script), &crate::DELIVERED_EVENTS, None).expect("the VM runs it");
    assert_eq!(events, ["OnDestructionStageChanged", "OnTrapHitStart", "OnTrapHitStop"].map(String::from));
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(REF) });
    world.insert_resource(pool);
    let reference = world.spawn();
    world.insert(reference, FormIdComponent(id));
    let instance = ScriptInstance { name: "VmRaisedScript".into(), status: 0, properties: Vec::new() };
    crate::attach_reference_script(&mut world, reference, &instance, events, scripts);
    let var = |world: &World, name: &str| {
        world.resource::<crate::QuestScriptVariables>().get(QuestFormId(REF), crate::QuestScriptVariables::property_key(name))
    };

    crate::raise_reference_event(
        &world,
        reference,
        "OnTrapHitStart",
        vec![
            VmStored::Form(TARGET),
            VmStored::Float(0.0),
            VmStored::Float(0.0),
            VmStored::Float(0.0),
            VmStored::Float(1.0),
            VmStored::Float(2.0),
            VmStored::Float(3.0),
            VmStored::Int(0),
            VmStored::Bool(true),
            VmStored::Int(4),
        ],
    );
    crate::raise_reference_event(&world, reference, "OnTrapHitStop", vec![VmStored::Form(TARGET)]);
    crate::raise_reference_event(&world, reference, "OnDestructionStageChanged", vec![VmStored::Int(-1), VmStored::Int(2)]);
    crate::reference_script_event_system(&world, 0.0);
    crate::fragment::fragment_continuation_system(&world, 0.0);
    assert_eq!(var(&world, "Hits"), Some(1.0), "the start's arguments bound by position");
    assert_eq!(var(&world, "Motion"), Some(4.0));
    assert_eq!(var(&world, "Stopped"), Some(1.0));
    assert_eq!(var(&world, "Stage"), Some(2.0));
}

/// A reference script the VM runs: `OnInit` once, `OnLoad` on every cell
/// load, `OnTriggerEnter` only in its auto state (the crossing moves it to
/// `Done`), and `OnActivate` — each through the event markers.
#[test]
fn reference_scripts_run_their_events() {
    const REF: u32 = 0x0001_0F10;
    let script = parsed_source(
        "ScriptName VmTriggerScript extends ObjectReference\n\
         ObjectReference Property Marker Auto\n\
         Int Property Count Auto\n\
         Event OnInit()\n Count = 10\n EndEvent\n\
         Event OnLoad()\n Count += 1\n EndEvent\n\
         Event OnActivate(ObjectReference akActionRef)\n Self.Disable()\n EndEvent\n\
         Auto State Waiting\n\
           Event OnTriggerEnter(ObjectReference akActionRef)\n\
             If akActionRef == Game.GetPlayer()\n GotoState(\"Done\")\n Marker.Enable()\n EndIf\n\
           EndEvent\n\
         EndState\n\
         State Done\n EndState\n",
    );
    let (events, scripts) =
        crate::check_reference_script(Arc::clone(&script), &crate::DELIVERED_EVENTS, None).expect("the VM runs it");
    assert_eq!(events, ["OnInit", "OnLoad", "OnActivate", "OnTriggerEnter"]);

    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(REF),
    });
    world.insert_resource(pool);
    let reference = world.spawn();
    world.insert(reference, FormIdComponent(id));
    world
        .resource_mut::<crate::fragment::ReferenceEnableState>()
        .set_enabled(MARKER, false);
    let instance = ScriptInstance {
        name: "VmTriggerScript".into(),
        status: 0,
        properties: vec![ScriptProperty {
            name: "Marker".into(),
            status: 1,
            value: PropertyValue::Object {
                form_id: MARKER,
                alias: -1,
            },
        }],
    };
    crate::attach_reference_script(&mut world, reference, &instance, events, scripts);

    let count = |world: &World| {
        world
            .resource::<crate::QuestScriptVariables>()
            .get(QuestFormId(REF), crate::QuestScriptVariables::property_key("Count"))
    };
    for expected in [11.0, 12.0] {
        world.insert(reference, crate::OnCellLoadEvent);
        crate::reference_script_event_system(&world, 0.0);
        world.remove::<crate::OnCellLoadEvent>(reference);
        assert_eq!(count(&world), Some(expected), "OnInit once, OnLoad each load");
    }

    for crossing in 0..2 {
        world.insert(
            reference,
            crate::OnTriggerEnterEvent {
                triggerers: vec![player],
            },
        );
        crate::reference_script_event_system(&world, 0.0);
        world.remove::<crate::OnTriggerEnterEvent>(reference);
        assert_eq!(
            crate::fragment::reference_enabled(&world, MARKER),
            crossing == 0,
            "only the auto state handles the crossing"
        );
        world
            .resource_mut::<crate::fragment::ReferenceEnableState>()
            .set_enabled(MARKER, false);
    }

    world.insert(reference, crate::ActivateEvent { activator: player });
    crate::reference_script_event_system(&world, 0.0);
    assert!(!crate::fragment::reference_enabled(&world, REF), "OnActivate disabled itself");

    // `Activate(akActionRef, true)` is default processing only: the script hears no
    // OnActivate, and the block flag is ignored (CK *Activate - ObjectReference*).
    world.resource_mut::<crate::fragment::ReferenceEnableState>().set_enabled(REF, true);
    world.remove::<crate::ActivateEvent>(reference);
    world.register::<crate::NativeActivation>();
    world.insert(reference, crate::ActivateEvent { activator: player });
    world.insert(reference, crate::NativeActivation);
    crate::reference_script_event_system(&world, 0.0);
    assert!(crate::fragment::reference_enabled(&world, REF), "no OnActivate for a default-processing activation");
    world.resource_mut::<crate::ReferenceActivationBlocks>().set_blocked(REF, true);
    assert!(!crate::default_activation_blocked(&world, reference), "the block flag is ignored");
    world.remove::<crate::NativeActivation>(reference);
    assert!(crate::default_activation_blocked(&world, reference), "...and stands for a plain activation");
}

/// An alias script the VM runs: `OnInit` then `OnAliasInit` when its quest
/// starts, the filled reference's `OnDeath`, and `OnAliasShutdown` when the
/// quest stops — all under one owner id per (quest, alias), with `Self` the
/// alias.
#[test]
fn alias_scripts_start_follow_their_reference_and_shut_down() {
    const ALIAS: i16 = 3;
    const FILLED: u32 = 0x0001_0F50;
    let script = parsed_source(
        "ScriptName VmAliasScript extends ReferenceAlias\n\
         ObjectReference Property Marker Auto\n\
         Int Property Count Auto\n\
         Event OnInit()\n Count = 5\n EndEvent\n\
         Event OnAliasInit()\n Count += 10\n EndEvent\n\
         Event OnAliasShutdown()\n Count += 100\n EndEvent\n\
         Event OnDeath(Actor akKiller)\n\
           If Self.GetReference() == akKiller || akKiller == Game.GetPlayer()\n\
             Marker.Enable()\n\
             Self.GetOwningQuest().SetStage(20)\n\
           EndIf\n\
         EndEvent\n",
    );
    let (events, scripts) =
        crate::check_alias_script(Arc::clone(&script), &crate::DELIVERED_ALIAS_EVENTS, None).expect("the VM runs it");
    assert_eq!(events, ["OnInit", "OnAliasInit", "OnAliasShutdown", "OnDeath"]);

    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    world.insert_resource(crate::QuestScriptObjects::default());
    world.insert_resource(crate::QuestScriptVariables::default());
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(FILLED),
    });
    world.insert_resource(pool);
    let filled = world.spawn();
    world.insert(filled, FormIdComponent(id));
    world.resource_mut::<crate::fragment::ReferenceEnableState>().set_enabled(MARKER, false);
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        for script in scripts {
            fragments.insert_vm_script(script);
        }
    }
    let instance = ScriptInstance {
        name: "VmAliasScript".into(),
        status: 0,
        properties: vec![ScriptProperty {
            name: "Marker".into(),
            status: 1,
            value: PropertyValue::Object {
                form_id: MARKER,
                alias: -1,
            },
        }],
    };
    world.resource_mut::<crate::QuestAliasScripts>().set_checked(
        Q,
        vec![crate::AliasScript {
            alias: ALIAS,
            script: "VmAliasScript".into(),
            vmad: Arc::new(ScriptInstanceData {
                version: 5,
                object_format: 2,
                scripts: vec![instance],
            }),
            events,
        }],
    );
    world.resource_mut::<crate::SceneActorBindings>().bind(Q, i32::from(ALIAS), filled);

    // The quest has not started: nothing runs.
    crate::alias_script_event_system(&world, 0.0);
    assert!(world.resource::<crate::AliasScriptOwners>().owner_of(Q, ALIAS).is_none());

    world.resource_mut::<QuestStageState>().start_quest(Q, None);
    crate::alias_script_event_system(&world, 0.0);
    let owner = world.resource::<crate::AliasScriptOwners>().owner_of(Q, ALIAS).expect("allocated at start");
    let count = |world: &World| {
        world
            .resource::<crate::QuestScriptVariables>()
            .get(QuestFormId(owner), crate::QuestScriptVariables::property_key("Count"))
    };
    assert_eq!(count(&world), Some(15.0), "OnInit then OnAliasInit");
    crate::alias_script_event_system(&world, 0.0);
    assert_eq!(count(&world), Some(15.0), "started once");

    // The filled reference dies: the alias script's OnDeath runs.
    world.insert(filled, crate::OnDeathEvent { killer: Some(player) });
    crate::reference_script_event_system(&world, 0.0);
    assert!(crate::fragment::reference_enabled(&world, MARKER));
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);

    // A stopped quest shuts its alias scripts down, once.
    world.resource_mut::<QuestStageState>().stop(Q);
    crate::alias_script_event_system(&world, 0.0);
    assert_eq!(count(&world), Some(115.0));
    crate::alias_script_event_system(&world, 0.0);
    assert_eq!(count(&world), Some(115.0), "shut down once");
}

/// A world with a quest's own script checked and installed the way the host does it.
fn world_with_quest_script(script: Arc<Script>, library: Option<&dyn CheckLibrary>, running: bool) -> Option<World> {
    world_with_quest_script_extending(script, library, running, Vec::new())
}

/// [`world_with_quest_script`] for a script whose `extends` chain the host recorded as `extends`.
fn world_with_quest_script_extending(
    script: Arc<Script>,
    library: Option<&dyn CheckLibrary>,
    running: bool,
    extends: Vec<String>,
) -> Option<World> {
    let (events, scripts) = crate::check_quest_script(Arc::clone(&script), &crate::DELIVERED_QUEST_EVENTS, library).ok()?;
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    world.insert_resource(crate::QuestScriptObjects::default());
    world.insert_resource(crate::QuestScriptVariables::default());
    world.init_resource::<crate::QuestOwnScripts>();
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        for script in scripts {
            fragments.insert_vm_script(script);
        }
    }
    let instance = ScriptInstance { name: script.name.node.0.clone(), status: 0, properties: Vec::new() };
    world.resource_mut::<crate::QuestOwnScripts>().set_checked(
        Q,
        vec![crate::QuestScript {
            script: script.name.node.0.clone(),
            vmad: Arc::new(ScriptInstanceData { version: 5, object_format: 2, scripts: vec![instance] }),
            events,
            extends,
        }],
    );
    if running {
        world.resource_mut::<QuestStageState>().start_quest(Q, None);
    }
    Some(world)
}

fn quest_property(world: &World, name: &str) -> Option<f32> {
    world.resource::<crate::QuestScriptVariables>().get(Q, crate::QuestScriptVariables::property_key(name))
}

/// CK *OnInit*: a quest's own script receives it "on game startup" — whether or not the quest runs —
/// and the engine delivers it once per save, so a second pass (a later frame, a load) does not re-run it.
#[test]
fn a_quest_script_receives_oninit_once_at_startup() {
    let script = parsed_source(
        "ScriptName VmQuestInit extends Quest\n\
         Int Property Count Auto\n\
         Event OnInit()\n Count += 7\n EndEvent\n",
    );
    for running in [false, true] {
        let world = world_with_quest_script(Arc::clone(&script), None, running).expect("the VM runs it");
        assert_eq!(quest_property(&world, "Count"), None);
        crate::quest_script_init_system(&world, 0.0);
        assert_eq!(quest_property(&world, "Count"), Some(7.0), "running: {running}");
        crate::quest_script_init_system(&world, 0.0);
        assert_eq!(quest_property(&world, "Count"), Some(7.0), "once per save, running: {running}");
    }
}

/// The gate is saved state (the quest's script variables): a quest whose variables a save restored
/// has had its `OnInit`, and one added to an existing save has not.
#[test]
fn a_quest_script_oninit_follows_the_saved_gate() {
    let script = parsed_source(
        "ScriptName VmQuestInitGate extends Quest\n\
         Int Property Count Auto\n\
         Event OnInit()\n Count += 1\n EndEvent\n",
    );
    let world = world_with_quest_script(Arc::clone(&script), None, false).expect("the VM runs it");
    let gate = crate::fragment::reference::initialized_key("VmQuestInitGate");
    world.resource_mut::<crate::QuestScriptVariables>().set(Q, gate, 1.0);
    crate::quest_script_init_system(&world, 0.0);
    assert_eq!(quest_property(&world, "Count"), None, "the save already recorded the OnInit");
}

/// An inherited `OnInit` runs (SkyUI's `SKI_ConfigBase` is the case), and a quest script handling no
/// delivered event is declined rather than run.
#[test]
fn a_quest_script_inherits_its_oninit_and_one_without_is_declined() {
    let base = parsed_source(
        "ScriptName VmQuestBase extends Quest\n\
         Int Property Count Auto\n\
         Event OnInit()\n Count += 3\n EndEvent\n",
    );
    let child = parsed_source("ScriptName VmQuestChild extends VmQuestBase\n");
    let scripts: HashMap<String, Arc<Script>> = [("vmquestbase".to_owned(), Arc::clone(&base))].into();
    let library = Installed(&scripts);
    let world = world_with_quest_script(Arc::clone(&child), Some(&library), false).expect("the inherited event runs");
    crate::quest_script_init_system(&world, 0.0);
    assert_eq!(quest_property(&world, "Count"), Some(3.0));

    let silent = parsed_source("ScriptName VmQuestSilent extends Quest\nFunction Helper()\nEndFunction\n");
    assert!(world_with_quest_script(silent, None, false).is_none(), "no OnInit: not a delivered script");
}

/// LMC-UI-002 — a SkyUI config script on the engine's `SKI_ConfigBase`: it registers from its `OnInit`
/// (`OnGameReload` → `OnConfigInit` → registration), lays its pages out on demand, and answers the
/// panel: a toggle by option id on the front page, a slider by state on a named page, whose dialog its
/// open event fills; an id from another page names nothing. After a load it registers again.
#[test]
fn a_skyui_config_script_registers_lays_out_and_answers_the_panel() {
    use crate::mcm::{McmAnswer, McmDialog, McmHost, McmKind};
    let base = crate::mcm::engine_script("SKI_ConfigBase").expect("the engine's SKI_ConfigBase parses");
    let config = parsed_source(
        "ScriptName VmMcmConfig extends SKI_ConfigBase\n\
         Bool Property Enabled Auto\n\
         Float Property Speed Auto\n\
         Int toggleId\n\
         Event OnConfigInit()\n ModName = \"VM MCM\"\n Pages = new String[1]\n Pages[0] = \"Movement\"\n Speed = 1.0\n EndEvent\n\
         Event OnPageReset(String page)\n\
          If page == \"\"\n SetCursorFillMode(TOP_TO_BOTTOM)\n AddHeaderOption(\"General\")\n toggleId = AddToggleOption(\"Enabled\", Enabled)\n\
          Else\n AddSliderOptionST(\"SpeedState\", \"Speed\", Speed, \"{1}\")\n EndIf\n\
         EndEvent\n\
         Event OnOptionSelect(Int option)\n If option == toggleId\n Enabled = !Enabled\n SetToggleOptionValue(option, Enabled)\n EndIf\n EndEvent\n\
         State SpeedState\n\
          Event OnSliderOpenST()\n SetSliderDialogStartValue(Speed)\n SetSliderDialogRange(0.5, 3.0)\n SetSliderDialogInterval(0.5)\n EndEvent\n\
          Event OnSliderAcceptST(Float value)\n Speed = value\n SetSliderOptionValueST(Speed, \"{1}\")\n EndEvent\n\
         EndState\n",
    );
    let scripts: HashMap<String, Arc<Script>> = [(crate::mcm::CONFIG_BASE.to_owned(), Arc::clone(&base))].into();
    let library = Installed(&scripts);
    let world = world_with_quest_script_extending(config, Some(&library), true, vec![crate::mcm::CONFIG_BASE.into()])
        .unwrap_or_else(|| panic!("declined: {:?}", crate::check_quest_script(parsed_source("ScriptName X extends Quest\n"), &[], None)));

    crate::quest_script_init_system(&world, 0.0);
    {
        let host = world.resource::<McmHost>();
        assert_eq!(host.mods.len(), 1, "registered from OnInit");
        assert_eq!(host.mods[0].name, "VM MCM");
        assert_eq!(host.mods[0].pages, ["Movement"]);
    }

    crate::mcm::open(&world, 0).expect("opens");
    let toggle = {
        let host = world.resource::<McmHost>();
        let page = host.open.as_ref().unwrap();
        assert_eq!(page.title, "VM MCM", "the front page is titled with the mod's name");
        assert_eq!(page.cells[0].as_ref().map(|o| o.kind), Some(McmKind::Header));
        assert!(page.cells[1].is_none(), "top to bottom: the next option is a row down");
        assert_eq!(page.cells[2].as_ref().map(|o| (o.kind, o.number)), Some((McmKind::Toggle, 0.0)));
        page.option_id(2)
    };
    assert_eq!(toggle, 2, "the front page's ids are its cell indices");
    crate::mcm::activate(&world, toggle).expect("selects");
    assert_eq!(quest_property(&world, "Enabled"), Some(1.0), "OnOptionSelect flipped the setting");
    assert_eq!(world.resource::<McmHost>().open.as_ref().unwrap().option(toggle).unwrap().number, 1.0, "and set the toggle");

    crate::mcm::select_page(&world, Some(0)).expect("shows the page");
    assert!(crate::mcm::activate(&world, toggle).is_err(), "the front page's id names nothing here");
    let slider = 0x100;
    {
        let host = world.resource::<McmHost>();
        let option = host.open.as_ref().unwrap().option(slider).expect("page 1's first cell is 0x100");
        assert_eq!((option.kind, option.state.as_str(), option.value_text()), (McmKind::Slider, "SpeedState", "1.0".to_owned()));
    }
    crate::mcm::activate(&world, slider).expect("opens the dialog");
    assert_eq!(
        world.resource::<McmHost>().open.as_ref().unwrap().dialog,
        Some((slider, McmDialog::Slider { start: 1.0, default: 1.0, min: 0.5, max: 3.0, interval: 0.5 })),
        "OnSliderOpenST ran in its state and filled the dialog"
    );
    crate::mcm::accept(&world, slider, McmAnswer::Slider(2.5)).expect("accepts");
    assert_eq!(quest_property(&world, "Speed"), Some(2.5));
    {
        let host = world.resource::<McmHost>();
        let page = host.open.as_ref().unwrap();
        assert_eq!(page.option(slider).unwrap().value_text(), "2.5");
        assert!(page.dialog.is_none());
    }
    crate::mcm::close(&world).expect("closes");

    // A load: the session's registrations are gone, and the config script registers again from its
    // OnGameReload (its OnInit already ran in this save).
    world.resource_mut::<McmHost>().reset_session();
    crate::mcm::mcm_registration_system(&world, 0.0);
    assert_eq!(world.resource::<McmHost>().mods.len(), 1);
    assert_eq!(quest_property(&world, "Speed"), Some(2.5), "OnConfigInit does not run again");
}

/// SkyUI's `ShowMessage` (a box wrapper on the engine's base): the option's handler raises a box with
/// the message and Accept / Cancel, waits for the answer, and replays its `If` with it — accepted runs
/// the branch, cancelled does not.
#[test]
fn a_config_script_waits_for_show_message_and_branches_on_it() {
    use crate::mcm::McmHost;
    let base = crate::mcm::engine_script("SKI_ConfigBase").unwrap();
    let source = "ScriptName VmMcmAsks extends SKI_ConfigBase\n\
         Bool Property Enabled Auto\n\
         Event OnConfigInit()\n ModName = \"Asks\"\n EndEvent\n\
         Event OnPageReset(String page)\n AddToggleOption(\"Enabled\", Enabled)\n EndEvent\n\
         Event OnOptionSelect(Int option)\n\
          If ShowMessage(\"Turn it on?\")\n Enabled = True\n SetToggleOptionValue(option, Enabled)\n EndIf\n\
         EndEvent\n";
    let run = |button: usize| {
        let scripts: HashMap<String, Arc<Script>> = [(crate::mcm::CONFIG_BASE.to_owned(), Arc::clone(&base))].into();
        let world = world_with_quest_script_extending(parsed_source(source), Some(&Installed(&scripts)), true, vec![crate::mcm::CONFIG_BASE.into()])
            .expect("a config script calling ShowMessage runs");
        crate::quest_script_init_system(&world, 0.0);
        crate::mcm::open(&world, 0).expect("opens");
        crate::mcm::activate(&world, 0).expect("selects");
        crate::fragment::fragment_continuation_system(&world, 0.0);
        let raised = std::mem::take(&mut world.resource_mut::<crate::PendingScriptMessages>().0);
        assert_eq!(raised.len(), 1, "the box is up");
        assert_eq!((raised[0].text.as_str(), raised[0].buttons.as_slice()), ("Turn it on?", ["$Accept".to_owned(), "$Cancel".to_owned()].as_slice()));
        let crate::MessageAnswer::Papyrus(ticket) = raised[0].answer else { panic!("{:?}", raised[0].answer) };
        crate::fragment::fragment_continuation_system(&world, 1.0);
        assert_eq!(quest_property(&world, "Enabled"), None, "the handler waits for the answer");
        crate::messages::record_papyrus_answer(&world, ticket, button);
        crate::fragment::fragment_continuation_system(&world, 0.0);
        assert!(world.resource::<crate::PendingScriptMessages>().0.is_empty(), "the replay reads the answer; no second box");
        let shown = world.resource::<McmHost>().open.as_ref().unwrap().option(0).unwrap().number;
        (quest_property(&world, "Enabled"), shown)
    };
    assert_eq!(run(0), (Some(1.0), 1.0), "accepted: the branch ran and set the toggle");
    assert_eq!(run(1), (None, 0.0), "cancelled: it did not");
}

/// CK *OnInit*: "All your variables and properties will be reset before OnInit is called. For Quests
/// and Aliases this will happen when your quest starts" — unless it is Run Once. The reset lands before
/// the startup stage's fragment, which then writes over the reset value; `OnInit` runs in between.
#[test]
fn a_quest_resets_its_script_state_as_it_starts_unless_run_once() {
    let script = parsed_source(
        "ScriptName VmQuestReset extends Quest\n\
         Int Property Count Auto\n\
         Int Property Kept Auto\n\
         Event OnInit()\n Count += 1\n EndEvent\n\
         Function Fragment_10()\n Kept += 100\n EndFunction\n",
    );
    let run = |flags: u16| {
        let mut world = world_with_quest_script(Arc::clone(&script), None, false).expect("the VM runs it");
        let vmad = ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance { name: "VmQuestReset".into(), status: 0, properties: Vec::new() }],
        };
        crate::quest_stages::install_start_game_quests(
            &mut world,
            [byroredux_plugin::esm::records::QustRecord { form_id: Q.0, quest_flags: flags, ..Default::default() }],
        );
        world.insert_resource(crate::QuestScriptInstances(Arc::new(HashMap::from([(Q, vmad.clone())]))));
        {
            let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
            fragments.insert_vmad(Q, vmad);
            fragments.insert(
                Q,
                10,
                vec![Effect::Interpret { script: "VmQuestReset".into(), function: "Fragment_10".into(), args: Vec::new(), resume: None }],
            );
        }
        // A save's state from an earlier run of the quest.
        crate::quest_script_init_system(&world, 0.0);
        {
            let mut variables = world.resource_mut::<crate::QuestScriptVariables>();
            variables.set(Q, crate::QuestScriptVariables::property_key("Count"), 5.0);
            variables.set(Q, crate::QuestScriptVariables::property_key("Kept"), 9.0);
        }
        world.resource_mut::<QuestStageState>().start_quest(Q, Some(10));
        crate::fragment::quest_fragment_dispatch_system(&world);
        crate::quest_script_init_system(&world, 0.0);
        (quest_property(&world, "Count"), quest_property(&world, "Kept"))
    };
    assert_eq!(run(0), (Some(1.0), Some(100.0)), "reset, OnInit once more, then the startup stage");
    assert_eq!(
        run(byroredux_plugin::esm::records::QUEST_FLAG_RUN_ONCE),
        (Some(5.0), Some(109.0)),
        "a Run Once quest keeps its state"
    );
}

/// A script whose `OnInit` the VM cannot run is declined whole, so it never half-runs.
#[test]
fn a_quest_script_with_an_unmodelled_oninit_is_declined() {
    let script = parsed_source(
        "ScriptName VmQuestUnmodelled extends Quest\n\
         Event OnInit()\n VmNoSuchLibrary.DoThing()\n EndEvent\n",
    );
    assert!(crate::check_quest_script(script, &crate::DELIVERED_QUEST_EVENTS, None).is_err());
}

/// A Fallout 4 alias script registers for its own hit in `OnAliasInit`; the
/// `OnHit(akTarget, akAggressor, …)` it declares is not delivered automatically,
/// runs once for the first matching hit, and is delivered the nine arguments of
/// its signature.
#[test]
fn hit_events_are_registered_single_shot_and_filtered() {
    const ALIAS: i16 = 5;
    const FILLED: u32 = 0x0001_0F60;
    let script = parsed_source(
        "ScriptName VmHitAlias extends ReferenceAlias\n\
         ObjectReference Property Marker Auto\n\
         Int Property Hits Auto\n\
         Event OnAliasInit()\n\
           RegisterForHitEvent(Self as ScriptObject, None, None, None, -1, 0, -1, -1, True)\n\
         EndEvent\n\
         Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, \
             Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)\n\
           Hits += 1\n\
           If akAggressor == Game.GetPlayer() && abPowerAttack\n Marker.Enable()\n EndIf\n\
         EndEvent\n",
    );
    let (events, scripts) =
        crate::check_alias_script(Arc::clone(&script), &crate::DELIVERED_ALIAS_EVENTS, None).expect("the VM runs it");
    assert_eq!(events, ["OnAliasInit"], "OnHit is registration-based: not delivered automatically");

    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    world.insert_resource(crate::QuestScriptObjects::default());
    world.insert_resource(crate::QuestScriptVariables::default());
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Fallout4.esm"),
        local: LocalFormId(FILLED),
    });
    world.insert_resource(pool);
    let filled = world.spawn();
    world.insert(filled, FormIdComponent(id));
    world.resource_mut::<crate::fragment::ReferenceEnableState>().set_enabled(MARKER, false);
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        for script in scripts {
            fragments.insert_vm_script(script);
        }
    }
    let instance = ScriptInstance {
        name: "VmHitAlias".into(),
        status: 0,
        properties: vec![ScriptProperty {
            name: "Marker".into(),
            status: 1,
            value: PropertyValue::Object {
                form_id: MARKER,
                alias: -1,
            },
        }],
    };
    world.resource_mut::<crate::QuestAliasScripts>().set_checked(
        Q,
        vec![crate::AliasScript {
            alias: ALIAS,
            script: "VmHitAlias".into(),
            vmad: Arc::new(ScriptInstanceData {
                version: 6,
                object_format: 2,
                scripts: vec![instance],
            }),
            events,
        }],
    );
    world.resource_mut::<crate::SceneActorBindings>().bind(Q, i32::from(ALIAS), filled);
    world.resource_mut::<QuestStageState>().start_quest(Q, None);
    crate::alias_script_event_system(&world, 0.0);
    assert_eq!(world.resource::<crate::HitRegistrations>().entries().len(), 1, "registered in OnAliasInit");

    let owner = world.resource::<crate::AliasScriptOwners>().owner_of(Q, ALIAS).unwrap();
    let hits = |world: &World| {
        world
            .resource::<crate::QuestScriptVariables>()
            .get(QuestFormId(owner), crate::QuestScriptVariables::property_key("Hits"))
    };
    let strike = |world: &mut World, sneak: bool| {
        world.insert(
            filled,
            crate::HitEvent {
                aggressor: player,
                source: filled,
                projectile: filled,
                damage: 3.0,
                power_attack: true,
                sneak_attack: sneak,
                bash_attack: false,
                blocked: false,
                arrow: false,
                ignores_resistance: false,
            },
        );
        // The automatic delivery leaves it alone, the registration takes it.
        crate::reference_script_event_system(world, 0.0);
        crate::reference_script_hit_system(world, 0.0);
        crate::hit_event_system(world, 0.0);
        world.remove::<crate::HitEvent>(filled);
    };
    // A sneak attack fails the aiSneakFilter 0: no delivery, still registered.
    strike(&mut world, true);
    assert_eq!(hits(&world), None);
    assert_eq!(world.resource::<crate::HitRegistrations>().entries().len(), 1);
    // The first matching hit is delivered with its signature's arguments, once.
    strike(&mut world, false);
    assert_eq!(hits(&world), Some(1.0));
    assert!(crate::fragment::reference_enabled(&world, MARKER), "akAggressor and abPowerAttack arrived");
    assert!(world.resource::<crate::HitRegistrations>().entries().is_empty(), "single shot");
    strike(&mut world, false);
    assert_eq!(hits(&world), Some(1.0), "re-registration is the script's to do");

    // #5987 — a hit the frame's budget cannot start is deferred, not lost: the
    // marker is gone by the next frame and the registration was one-shot.
    let registration = {
        // Re-arm the (consumed) registration the way the script would.
        use crate::fragment::{HitRegistrations, HitRequest};
        let mut registrations = world.resource_mut::<HitRegistrations>();
        registrations.request(
            owner,
            "VmHitAlias",
            &HitRequest::Register {
                target: crate::fragment::DistanceTarget::Alias { quest: Q.0, alias: ALIAS },
                filter: crate::fragment::HitFilter::default(),
            },
        );
        registrations.entries().len()
    };
    assert_eq!(registration, 1);
    budgeted(&mut world, 0);
    strike(&mut world, false);
    assert_eq!(hits(&world), Some(1.0), "the exhausted frame did not start the run");
    assert!(world.resource::<crate::HitRegistrations>().entries().is_empty(), "the registration fired");
    assert_eq!(world.resource::<crate::FragmentExecutionQueue>().len(), 1, "the hit's run is queued");
    world.insert_resource(crate::VmFrameBudget::default());
    next_frame(&world);
    assert_eq!(hits(&world), Some(2.0), "the deferred hit ran on a later frame");
}

/// `OnEnterBleedout` arrives when the actor goes into bleedout, `OnDying` with
/// the death ahead of `OnDeath`; a cell unload raises `OnCellDetach` then
/// `OnUnload` for the references about to despawn.
#[test]
fn lifecycle_events_reach_reference_scripts() {
    const REF: u32 = 0x0001_0F14;
    let script = parsed_source(
        "ScriptName VmLifecycleScript extends Actor\n\
         Int Property Trail Auto\n\
         Event OnEnterBleedout()\n Trail = Trail * 10 + 5\n EndEvent\n\
         Event OnDying(Actor akKiller)\n Trail = Trail * 10 + 1\n EndEvent\n\
         Event OnDeath(Actor akKiller)\n Trail = Trail * 10 + 2\n EndEvent\n\
         Event OnCellDetach()\n Trail = Trail * 10 + 3\n EndEvent\n\
         Event OnUnload()\n Trail = Trail * 10 + 4\n EndEvent\n",
    );
    let (events, scripts) =
        crate::check_reference_script(Arc::clone(&script), &crate::DELIVERED_EVENTS, None).expect("runs");
    assert_eq!(events, ["OnEnterBleedout", "OnDying", "OnDeath", "OnCellDetach", "OnUnload"]);
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(REF),
    });
    world.insert_resource(pool);
    let actor = world.spawn();
    world.insert(actor, FormIdComponent(id));
    let instance = ScriptInstance {
        name: "VmLifecycleScript".into(),
        status: 0,
        properties: Vec::new(),
    };
    crate::attach_reference_script(&mut world, actor, &instance, events, scripts);
    let trail = |world: &World| {
        world
            .resource::<crate::QuestScriptVariables>()
            .get(QuestFormId(REF), crate::QuestScriptVariables::property_key("Trail"))
    };
    world.insert(actor, crate::bleedout::Bleedout { remaining_secs: Some(20.0) });
    crate::reference_script_event_system(&world, 0.0);
    assert_eq!(trail(&world), Some(5.0), "entering bleedout");
    crate::reference_script_event_system(&world, 0.0);
    assert_eq!(trail(&world), Some(5.0), "once");
    world.insert(actor, crate::OnDeathEvent { killer: Some(player) });
    crate::reference_script_event_system(&world, 0.0);
    assert_eq!(trail(&world), Some(512.0), "OnDying then OnDeath");
    crate::raise_unload_events(&world, &[actor]);
    assert_eq!(trail(&world), Some(512.0), "delivered with the next frame's events");
    crate::reference_script_event_system(&world, 0.0);
    assert_eq!(trail(&world), Some(51234.0), "OnCellDetach then OnUnload");
}

/// #6083 — a save load's own cell teardown queues `OnCellDetach` /
/// `OnUnload` for the outgoing session; the load discards them rather than
/// run those handlers against the restored state, while an ordinary unload
/// still delivers them.
#[test]
fn a_load_discards_the_unload_events_its_own_teardown_raised() {
    const REF: u32 = 0x0001_0F16;
    let script = parsed_source(
        "ScriptName VmLoadUnloadScript extends Actor\n\
         Int Property Trail Auto\n\
         Event OnCellDetach()\n Trail = Trail * 10 + 3\n EndEvent\n\
         Event OnUnload()\n Trail = Trail * 10 + 4\n EndEvent\n",
    );
    let (events, scripts) =
        crate::check_reference_script(Arc::clone(&script), &crate::DELIVERED_EVENTS, None).expect("runs");
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(REF),
    });
    world.insert_resource(pool);
    let actor = world.spawn();
    world.insert(actor, FormIdComponent(id));
    let instance = ScriptInstance {
        name: "VmLoadUnloadScript".into(),
        status: 0,
        properties: Vec::new(),
    };
    crate::attach_reference_script(&mut world, actor, &instance, events, scripts);
    let trail = |world: &World| {
        world
            .resource::<crate::QuestScriptVariables>()
            .get(QuestFormId(REF), crate::QuestScriptVariables::property_key("Trail"))
    };

    crate::raise_unload_events(&world, &[actor]);
    crate::discard_pending_unload_deliveries(&world);
    crate::reference_script_event_system(&world, 0.0);
    assert_eq!(trail(&world), None, "the load's teardown events never ran");

    crate::raise_unload_events(&world, &[actor]);
    crate::reference_script_event_system(&world, 0.0);
    assert_eq!(trail(&world), Some(34.0), "an ordinary unload still delivers");
}

/// #6083 — after a load `ActorWatch` is re-baselined: an actor already in
/// bleedout or combat in the loaded world did not just enter it, and the
/// player's location (republished by a system, not saved) is a baseline again
/// rather than a move from the outgoing timeline's.
#[test]
fn a_load_rebaselines_the_actor_watch() {
    const REF: u32 = 0x0001_0F17;
    const LOC_A: u32 = 0x0001_0F93;
    const LOC_B: u32 = 0x0001_0F94;
    let script = parsed_source(
        "ScriptName VmLoadWatchScript extends Actor\n\
         Int Property Bleeds Auto\n\
         Int Property Moves Auto\n\
         Event OnEnterBleedout()\n Bleeds += 1\n EndEvent\n\
         Event OnLocationChange(Location akOldLoc, Location akNewLoc)\n Moves += 1\n EndEvent\n",
    );
    let (events, scripts) =
        crate::check_reference_script(Arc::clone(&script), &crate::DELIVERED_EVENTS, None).expect("runs");
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(REF),
    });
    world.insert_resource(pool);
    let actor = world.spawn();
    world.insert(actor, FormIdComponent(id));
    let instance = ScriptInstance {
        name: "VmLoadWatchScript".into(),
        status: 0,
        properties: Vec::new(),
    };
    crate::attach_reference_script(&mut world, actor, &instance, events.clone(), scripts.clone());
    crate::attach_reference_script(&mut world, player, &instance, events, scripts);
    let value = |world: &World, owner: u32, name: &str| {
        world
            .resource::<crate::QuestScriptVariables>()
            .get(QuestFormId(owner), crate::QuestScriptVariables::property_key(name))
    };

    // The outgoing session: the player stood at LOC_A, the actor was upright.
    world.insert_resource(crate::location::PlayerLocation(Some(LOC_A)));
    crate::reference_script_event_system(&world, 0.0);

    // The load: the restored actor is already bleeding out, and the system
    // republishes the player's location (here: already at LOC_B).
    world.insert(actor, crate::bleedout::Bleedout { remaining_secs: Some(20.0) });
    world.insert_resource(crate::location::PlayerLocation(Some(LOC_B)));
    crate::rebaseline_actor_watch(&world);
    crate::reference_script_event_system(&world, 0.0);
    assert_eq!(value(&world, REF, "Bleeds"), None, "already bleeding is not an entry");
    assert_eq!(value(&world, 0x14, "Moves"), None, "the loaded location is a baseline");

    // Real changes after the load still deliver.
    world.insert_resource(crate::location::PlayerLocation(Some(LOC_A)));
    crate::reference_script_event_system(&world, 0.0);
    assert_eq!(value(&world, 0x14, "Moves"), Some(1.0));
}

/// `OnCombatStateChanged(akTarget, aeCombatState)` follows an actor's combat
/// target (1 in combat, 0 left); the player's `OnLocationChange(akOldLoc,
/// akNewLoc)` follows `PlayerLocation`, its first sight a baseline.
#[test]
fn combat_state_and_location_changes_reach_reference_scripts() {
    const REF: u32 = 0x0001_0F15;
    const LOC_A: u32 = 0x0001_0F91;
    const LOC_B: u32 = 0x0001_0F92;
    let script = parsed_source(
        "ScriptName VmWatchScript extends Actor\n\
         Int Property Combat Auto\n\
         Int Property Moves Auto\n\
         Event OnCombatStateChanged(Actor akTarget, Int aeCombatState)\n\
           If aeCombatState == 1 && akTarget == Game.GetPlayer()\n Combat = 1\n\
           ElseIf aeCombatState == 0 && !akTarget\n Combat = 2\n EndIf\n\
         EndEvent\n\
         Event OnLocationChange(Location akOldLoc, Location akNewLoc)\n Moves += 1\n EndEvent\n",
    );
    let (events, scripts) =
        crate::check_reference_script(Arc::clone(&script), &crate::DELIVERED_EVENTS, None).expect("runs");
    assert_eq!(events, ["OnCombatStateChanged", "OnLocationChange"]);
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(REF),
    });
    world.insert_resource(pool);
    let actor = world.spawn();
    world.insert(actor, FormIdComponent(id));
    let instance = ScriptInstance {
        name: "VmWatchScript".into(),
        status: 0,
        properties: Vec::new(),
    };
    crate::attach_reference_script(&mut world, actor, &instance, events.clone(), scripts.clone());
    crate::attach_reference_script(&mut world, player, &instance, events, scripts);
    world.insert_resource(crate::location::PlayerLocation(Some(LOC_A)));
    let value = |world: &World, owner: u32, name: &str| {
        world
            .resource::<crate::QuestScriptVariables>()
            .get(QuestFormId(owner), crate::QuestScriptVariables::property_key(name))
    };
    crate::reference_script_event_system(&world, 0.0);
    assert_eq!(value(&world, 0x14, "Moves"), None, "the first location is a baseline");
    world.insert_resource(crate::location::PlayerLocation(Some(LOC_B)));
    crate::reference_script_event_system(&world, 0.0);
    assert_eq!(value(&world, 0x14, "Moves"), Some(1.0));
    crate::reference_script_event_system(&world, 0.0);
    assert_eq!(value(&world, 0x14, "Moves"), Some(1.0), "once per change");

    world.register::<crate::AiCombatState>();
    world.insert(actor, crate::AiCombatState { target: player, attack_cooldown_remaining: 0.0 });
    crate::reference_script_event_system(&world, 0.0);
    assert_eq!(value(&world, REF, "Combat"), Some(1.0), "entered combat with the player");
    world.remove::<crate::AiCombatState>(actor);
    crate::reference_script_event_system(&world, 0.0);
    assert_eq!(value(&world, REF, "Combat"), Some(2.0), "left combat");
}

/// `RegisterForUpdate` delivers `OnUpdate` every interval until the handler
/// unregisters; the check accepts the registration because the handler runs.
#[test]
fn periodic_updates_run_until_unregistered() {
    let script = parsed_source(
        "ScriptName QF_VmUpdates extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Int Ticks\n\
         Function Fragment_1()\n RegisterForUpdate(1.0)\n EndFunction\n\
         Event OnUpdate()\n\
           Ticks += 1\n\
           If Ticks == 3\n UnregisterForUpdate()\n Marker.Enable()\n EndIf\n\
         EndEvent\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.insert_resource(crate::QuestScriptVariables::default());
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    let ticks = |world: &World| {
        world
            .resource::<crate::QuestScriptVariables>()
            .get(Q, crate::QuestScriptVariables::property_key("Ticks"))
    };
    crate::script_update_system(&world, 0.5, None);
    assert_eq!(ticks(&world), None, "not due yet");
    for expected in [1.0, 2.0, 3.0, 3.0] {
        crate::script_update_system(&world, 1.0, None);
        assert_eq!(ticks(&world), Some(expected));
    }
    assert!(crate::fragment::reference_enabled(&world, MARKER));
    assert!(world.resource::<crate::ScriptUpdates>().registrations().is_empty());
}

/// A registration whose handler the VM cannot run does not fail the code
/// that registers; the delivery skips the handler whole — it never runs the
/// statements before the one it cannot.
#[test]
fn a_handler_the_vm_cannot_run_is_skipped_whole() {
    let script = parsed_source(
        "ScriptName QF_VmBadUpdate extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n RegisterForSingleUpdate(1.0)\n EndFunction\n\
         Event OnUpdate()\n Marker.Enable()\n Debug.SendAnimationEvent(None, \"x\")\n EndEvent\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let world = world_with_fragment(script);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    crate::script_update_system(&world, 2.0, None);
    assert!(!crate::fragment::reference_enabled(&world, MARKER), "no partial run");
}

/// A reference's `RegisterForSingleUpdateGameTime` counts game hours from
/// the first clock the host reports, and delivers once.
#[test]
fn game_time_updates_count_game_hours() {
    const REF: u32 = 0x0001_0F11;
    let script = parsed_source(
        "ScriptName VmGameTimeScript extends ObjectReference\n\
         Event OnLoad()\n RegisterForSingleUpdateGameTime(2.0)\n EndEvent\n\
         Event OnUpdateGameTime()\n Self.Disable()\n EndEvent\n",
    );
    let (events, scripts) =
        crate::check_reference_script(Arc::clone(&script), &crate::DELIVERED_EVENTS, None).expect("runs");
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(REF),
    });
    world.insert_resource(pool);
    let reference = world.spawn();
    world.insert(reference, FormIdComponent(id));
    let instance = ScriptInstance {
        name: "VmGameTimeScript".into(),
        status: 0,
        properties: Vec::new(),
    };
    crate::attach_reference_script(&mut world, reference, &instance, events, scripts);
    world.insert(reference, crate::OnCellLoadEvent);
    crate::reference_script_event_system(&world, 0.0);
    // The registration predates the first reported clock: it arms at 10 h.
    crate::script_update_system(&world, 0.1, Some(10.0));
    crate::script_update_system(&world, 0.1, Some(11.5));
    assert!(crate::fragment::reference_enabled(&world, REF), "1.5 game hours in");
    crate::script_update_system(&world, 0.1, Some(12.0));
    assert!(!crate::fragment::reference_enabled(&world, REF));
    assert!(world.resource::<crate::ScriptUpdates>().registrations().is_empty(), "a single update");
}

/// #5997 — a reference attached with `OnInitEvent` alone (no 3D) is sent `OnInit`
/// and not `OnLoad`; `OnCellLoadEvent` (its 3D arrived) sends `OnLoad` too, and
/// `OnInit` still runs only once.
#[test]
fn an_identity_only_reference_gets_on_init_but_not_on_load() {
    const INIT_REF: u32 = 0x0001_0F21;
    const LOAD_REF: u32 = 0x0001_0F22;
    let init_script = parsed_source(
        "ScriptName VmIdentityInit extends ObjectReference\n\
         Event OnInit()\n Self.Disable()\n EndEvent\n",
    );
    let load_script = parsed_source(
        "ScriptName VmIdentityLoad extends ObjectReference\n\
         Event OnLoad()\n Self.Disable()\n EndEvent\n",
    );
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let mut attach = |world: &mut World, local: u32, name: &str, script: Arc<Script>| {
        let (events, scripts) =
            crate::check_reference_script(script, &crate::DELIVERED_EVENTS, None).expect("runs");
        let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(local) });
        let reference = world.spawn();
        world.insert(reference, FormIdComponent(id));
        let instance = ScriptInstance { name: name.into(), status: 0, properties: Vec::new() };
        crate::attach_reference_script(world, reference, &instance, events, scripts);
        world.insert(reference, crate::OnInitEvent);
        reference
    };
    let init_ref = attach(&mut world, INIT_REF, "VmIdentityInit", init_script);
    let load_ref = attach(&mut world, LOAD_REF, "VmIdentityLoad", load_script);
    world.insert_resource(pool);

    crate::reference_script_event_system(&world, 0.0);
    assert!(!crate::fragment::reference_enabled(&world, INIT_REF), "OnInit ran");
    assert!(crate::fragment::reference_enabled(&world, LOAD_REF), "no 3D: no OnLoad");

    // Its 3D arrives: the full set is delivered.
    world.remove::<crate::OnInitEvent>(load_ref);
    world.insert(load_ref, crate::OnCellLoadEvent);
    world.remove::<crate::OnInitEvent>(init_ref);
    crate::reference_script_event_system(&world, 0.0);
    assert!(!crate::fragment::reference_enabled(&world, LOAD_REF), "OnLoad ran with the 3D");
}

/// A `Native` declaration in a parent script the library serves (`Quest`,
/// `ObjectReference`, …) is not a script body: `Self.SetStage` still runs the
/// native, never the declaration's empty body.
#[test]
fn native_declarations_in_parent_scripts_stay_natives() {
    let quest = parsed_source(
        "ScriptName Quest extends Form\n\
         Bool Function SetStage(Int aiStage) Native\n",
    );
    let script = parsed_source(
        "ScriptName QF_VmNatives extends Quest\n\
         Function Fragment_1()\n Self.SetStage(20)\n EndFunction\n",
    );
    let scripts: HashMap<String, Arc<Script>> = [("quest".to_owned(), Arc::clone(&quest))].into();
    let library = Installed(&scripts);
    let needed = check_function_reason(Arc::clone(&script), "Fragment_1", Some(&library)).expect("runs");
    let mut world = world_with_fragment(script);
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        for needed in needed {
            fragments.insert_vm_script(needed);
        }
    }
    world.insert_resource(crate::QuestScriptVariables::default());
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// `Parent.Fn()` runs the version of the script the running function is defined
/// in extends — in the instance's state — and `Parent.OnInit()` past the
/// scripts to an engine class is the empty event.
#[test]
fn parent_calls_run_the_inherited_version() {
    let quest = parsed_source("ScriptName Quest extends Form\nBool Function SetStage(Int aiStage) Native\n");
    let base = parsed_source(
        "ScriptName VmParentBase extends Quest\n\
         Function Touch(Int aiStage)\n Self.SetStage(aiStage)\n EndFunction\n\
         Event OnInit()\n Self.SetStage(15)\n EndEvent\n",
    );
    let script = parsed_source(
        "ScriptName QF_VmParent extends VmParentBase\n\
         Function Touch(Int aiStage)\n Parent.Touch(aiStage + 5)\n EndFunction\n\
         Event OnInit()\n Parent.OnInit()\n EndEvent\n\
         Function Fragment_1()\n Touch(15)\n EndFunction\n\
         Function Fragment_2()\n Parent.Missing()\n EndFunction\n",
    );
    let scripts: HashMap<String, Arc<Script>> =
        [("quest".to_owned(), quest), ("vmparentbase".to_owned(), base)].into();
    let library = Installed(&scripts);
    let needed = check_function_reason(Arc::clone(&script), "Fragment_1", Some(&library)).expect("runs");
    assert!(
        check_function_reason(Arc::clone(&script), "Fragment_2", Some(&library)).is_err(),
        "a parent function that does not exist"
    );
    let mut world = world_with_fragment(script);
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        for needed in needed {
            fragments.insert_vm_script(needed);
        }
    }
    world.insert_resource(crate::QuestScriptVariables::default());
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20, "Touch(15) -> Parent.Touch(20)");
}

/// `SetKeywordData` / `GetKeywordData` on a location round-trip through the
/// saved ledger, and a read sees the write made earlier in the same run.
#[test]
fn location_keyword_data_round_trips() {
    const PLACE: u32 = 0x0001_0F72;
    const COST: u32 = 0x0001_0F73;
    let script = parsed_source(
        "ScriptName QF_VmKeywordData extends Quest\n\
         Location Property Place Auto\n\
         Keyword Property Cost Auto\n\
         Function Fragment_1()\n\
           Int count = 0\n\
           While count < 1\n count += 1\n EndWhile\n\
           Place.SetKeywordData(Cost, Place.GetKeywordData(Cost) + 25.0)\n\
           If Place.GetKeywordData(Cost) == 125.0\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let world = world_with_fragment(script);
    let object = |name: &str, form_id: u32| ScriptProperty {
        name: name.into(),
        status: 1,
        value: PropertyValue::Object { form_id, alias: -1 },
    };
    world.resource_mut::<crate::QuestStageFragments>().insert_vmad(
        Q,
        ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "QF_VmKeywordData".into(),
                status: 0,
                properties: vec![object("Place", PLACE), object("Cost", COST)],
            }],
        },
    );
    world.resource_mut::<crate::LocationKeywordData>().set(PLACE, COST, 100.0);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
    assert_eq!(crate::location_keyword_data(&world, PLACE, COST), 125.0);
}

/// `Weather.GetCurrentWeather()` answers the weather the host published, and
/// `GetClassification()` its `WTHR` classification (CK: 2 is Rainy).
#[test]
fn current_weather_and_its_classification() {
    const RAIN: u32 = 0x0001_0F74;
    let script = parsed_source(
        "ScriptName QF_VmWeather extends Quest\n\
         Function Fragment_1()\n\
           Weather now = Weather.GetCurrentWeather()\n\
           If now && now.GetClassification() == 2\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    let mut index = byroredux_plugin::esm::records::EsmIndex::default();
    index.weathers.insert(
        RAIN,
        byroredux_plugin::esm::records::WeatherRecord {
            form_id: RAIN,
            classification: byroredux_plugin::esm::records::weather::WTHR_RAINY,
            ..Default::default()
        },
    );
    crate::install_weather_classifications(&mut world, &index);
    // No weather published: the fragment's guard fails.
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 10);
    world.resource_mut::<crate::CurrentWeather>().0 = Some(RAIN);
    world.resource_mut::<QuestStageState>().set_stage(Q, 5);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// `FormList.GetSize` / `GetAt` / `HasForm` / `Find` read the `FLST` record's forms.
#[test]
fn form_list_reads() {
    const LIST: u32 = 0x0001_0F75;
    const FIRST: u32 = 0x0001_0F76;
    const SECOND: u32 = 0x0001_0F77;
    let script = parsed_source(
        "ScriptName QF_VmFormList extends Quest\n\
         FormList Property Types Auto\n\
         Form Property Wanted Auto\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n\
           If Types.GetSize() == 2 && Types.HasForm(Wanted) && Types.Find(Wanted) == 1 && Types.GetAt(0) == Types.GetAt(0) && !Types.GetAt(5)\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    let object = |name: &str, form_id: u32| ScriptProperty {
        name: name.into(),
        status: 1,
        value: PropertyValue::Object { form_id, alias: -1 },
    };
    world.resource_mut::<crate::QuestStageFragments>().insert_vmad(
        Q,
        ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "QF_VmFormList".into(),
                status: 0,
                properties: vec![object("Types", LIST), object("Wanted", SECOND), object("Marker", MARKER)],
            }],
        },
    );
    let mut index = byroredux_plugin::esm::records::EsmIndex::default();
    index.form_lists.insert(
        LIST,
        byroredux_plugin::esm::records::FlstRecord {
            form_id: LIST,
            entries: vec![FIRST, 0xFFFF_FFFF, SECOND],
            ..Default::default()
        },
    );
    crate::install_form_lists(&mut world, &index);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// `GetRace()` reads the actor's race; None for an actor without one.
#[test]
fn actor_race_reads_the_background() {
    const RACE: u32 = 0x0001_3740;
    let script = parsed_source(
        "ScriptName QF_VmRace extends Quest\n\
         Race Property Nord Auto\n\
         Function Fragment_1()\n\
           If Game.GetPlayer().GetRace() == Nord\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.resource_mut::<crate::QuestStageFragments>().insert_vmad(
        Q,
        ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "QF_VmRace".into(),
                status: 0,
                properties: vec![ScriptProperty {
                    name: "Nord".into(),
                    status: 1,
                    value: PropertyValue::Object { form_id: RACE, alias: -1 },
                }],
            }],
        },
    );
    let player = world.resource::<PapyrusPlayerEntity>().0;
    world.register::<byroredux_core::character::Background>();
    world.insert(player, byroredux_core::character::Background { race_form_id: RACE, ..Default::default() });
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// `GetFactionOwner()` is the reference's explicit faction owner, and None when
/// its owner is an actor base.
#[test]
fn faction_owner_is_the_explicit_owner() {
    const FACTION: u32 = 0x0001_0F78;
    let script = parsed_source(
        "ScriptName QF_VmOwner extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Faction Property Owners Auto\n\
         Function Fragment_1()\n\
           If Marker.GetFactionOwner() == Owners && !Marker.GetActorOwner()\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    let object = |name: &str, form_id: u32| ScriptProperty {
        name: name.into(),
        status: 1,
        value: PropertyValue::Object { form_id, alias: -1 },
    };
    world.resource_mut::<crate::QuestStageFragments>().insert_vmad(
        Q,
        ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "QF_VmOwner".into(),
                status: 0,
                properties: vec![object("Marker", MARKER), object("Owners", FACTION)],
            }],
        },
    );
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(MARKER),
    });
    world.insert_resource(pool);
    let marker = world.spawn();
    world.insert(marker, FormIdComponent(id));
    world.insert(marker, crate::ExplicitOwner(FACTION));
    let mut index = byroredux_plugin::esm::records::EsmIndex::default();
    index.factions.insert(
        FACTION,
        byroredux_plugin::esm::records::FactionRecord {
            form_id: FACTION,
            editor_id: String::new(),
            full_name: String::new(),
            flags: 0,
            relations: Vec::new(),
            ranks: Vec::new(),
            reputation: None,
            vendor: None,
            crime: None,
        },
    );
    crate::install_owner_kinds(&mut world, &index);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// `CalculateEncounterLevel(difficulty)`: the player's level lifted by the
/// reference's own zone, then scaled by the difficulty's game setting.
#[test]
fn calculate_encounter_level_follows_the_zone_and_difficulty() {
    use byroredux_plugin::esm::records::{EcznRecord, GameSetting, SettingValue};
    let script = parsed_source(
        "ScriptName QF_VmEncounter extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n\
           If Marker.CalculateEncounterLevel() == 20 && Marker.CalculateEncounterLevel(1) == 13\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.resource_mut::<crate::QuestStageFragments>().insert_vmad(
        Q,
        ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "QF_VmEncounter".into(),
                status: 0,
                properties: vec![ScriptProperty {
                    name: "Marker".into(),
                    status: 1,
                    value: PropertyValue::Object { form_id: MARKER, alias: -1 },
                }],
            }],
        },
    );
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(MARKER),
    });
    world.insert_resource(pool);
    let marker = world.spawn();
    world.insert(marker, FormIdComponent(id));
    world.insert(marker, crate::ExplicitEncounterZone(0x0001_E2E0));
    let player = world.resource::<PapyrusPlayerEntity>().0;
    world.register::<byroredux_core::character::CharacterLevel>();
    world.insert(player, byroredux_core::character::CharacterLevel { level: 5, xp: 0 });
    let mut index = byroredux_plugin::esm::records::EsmIndex::default();
    index.encounter_zones.insert(
        0x0001_E2E0,
        EcznRecord { form_id: 0x0001_E2E0, min_level: 20, ..Default::default() },
    );
    for (form_id, (name, value)) in (0x100u32..).zip([
        ("fLeveledActorMultEasy", 0.33),
        ("fLeveledActorMultMedium", 0.67),
        ("fLeveledActorMultHard", 1.0),
        ("fLeveledActorMultVeryHard", 1.25),
    ]) {
        index.game_settings.insert(
            form_id,
            GameSetting {
                form_id,
                editor_id: name.into(),
                value: SettingValue::Float(value),
            },
        );
    }
    crate::install_encounter_level_rules(&mut world, &index);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// `Actor.GetGoldAmount()` counts the actor's `Gold001`, and `GetBribeAmount()`
/// is the UESP formula over the two actors' levels, speech and morality.
#[test]
fn gold_amount_and_bribe_amount_read_the_actors() {
    use byroredux_core::ecs::components::{ActorValues, Inventory};
    use byroredux_plugin::esm::records::{AvifRecord, GameSetting, SettingValue};
    const SPEECH: u32 = 0x0000_0044;
    const MORALITY: u32 = 0x0000_0033;
    let script = parsed_source(
        "ScriptName QF_VmBribe extends Quest\n\
         Actor Property Guard Auto\n\
         Function Fragment_1()\n\
           If Guard.GetGoldAmount() == 7 && Guard.GetBribeAmount() == 35\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.resource_mut::<crate::QuestStageFragments>().insert_vmad(
        Q,
        ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "QF_VmBribe".into(),
                status: 0,
                properties: vec![ScriptProperty {
                    name: "Guard".into(),
                    status: 1,
                    value: PropertyValue::Object { form_id: MARKER, alias: -1 },
                }],
            }],
        },
    );
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(MARKER),
    });
    world.insert_resource(pool);
    let guard = world.spawn();
    world.insert(guard, FormIdComponent(id));
    world.insert(guard, byroredux_core::character::CharacterLevel { level: 10, xp: 0 });
    world.insert(
        guard,
        Inventory { items: vec![byroredux_core::ecs::components::ItemStack::new(0x0F, 7)] },
    );
    world.insert(guard, ActorValues::from_pairs([(SPEECH, 25.0), (MORALITY, 0.0)]));
    let player = world.resource::<PapyrusPlayerEntity>().0;
    world.register::<byroredux_core::character::CharacterLevel>();
    world.insert(player, byroredux_core::character::CharacterLevel { level: 10, xp: 0 });
    world.insert(player, ActorValues::from_pairs([(SPEECH, 25.0)]));
    world.insert_resource(crate::ScriptGame(byroredux_plugin::esm::reader::GameKind::Skyrim));
    let mut index = byroredux_plugin::esm::records::EsmIndex::default();
    for (form_id, editor_id) in [(SPEECH, "AVSpeechcraft"), (MORALITY, "AVMorality")] {
        index.actor_values.insert(form_id, AvifRecord { form_id, editor_id: editor_id.into(), ..Default::default() });
    }
    for (form_id, (name, value)) in (0x200u32..).zip([
        ("fBribeSpeechcraftMult", 0.1),
        ("fBribeScale", 0.5),
        ("fBribeCostCurve", 1.4),
    ]) {
        index.game_settings.insert(
            form_id,
            GameSetting { form_id, editor_id: name.into(), value: SettingValue::Float(value) },
        );
    }
    assert!(crate::install_bribe_rules(&mut world, &index));
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// `<Sound>.Play(akSource)` as a statement records the request for the host; a
/// receiver that is not declared a `Sound` (a `VisualEffect`) is declined.
#[test]
fn sound_play_records_a_request() {
    const SOUND: u32 = 0x0001_0F79;
    let script = parsed_source(
        "ScriptName QF_VmSound extends Quest\n\
         Sound Property Chime Auto\n\
         VisualEffect Property Glow Auto\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n Chime.Play(Marker)\n EndFunction\n\
         Function Fragment_2()\n Glow.Play(Marker)\n EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    assert!(check_function_reason(Arc::clone(&script), "Fragment_2", None).is_err());
    let world = world_with_fragment(script);
    let object = |name: &str, form_id: u32| ScriptProperty {
        name: name.into(),
        status: 1,
        value: PropertyValue::Object { form_id, alias: -1 },
    };
    world.resource_mut::<crate::QuestStageFragments>().insert_vmad(
        Q,
        ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "QF_VmSound".into(),
                status: 0,
                properties: vec![object("Chime", SOUND), object("Marker", MARKER)],
            }],
        },
    );
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(
        world.resource::<crate::PendingSoundPlays>().0,
        vec![crate::SoundPlay { sound: SOUND, source: MARKER }]
    );
}

/// `EnableLinkChain` / `DisableLinkChain` follow the unkeyworded links from the
/// reference, enabling (disabling) each, and stop at a loop.
#[test]
fn link_chains_are_followed() {
    const NEXT: u32 = 0x0001_0F7A;
    const LAST: u32 = 0x0001_0F7B;
    let script = parsed_source(
        "ScriptName QF_VmChain extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n Marker.EnableLinkChain()\n EndFunction\n\
         Function Fragment_2()\n Marker.DisableLinkChain()\n EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let world = world_with_fragment(script);
    {
        let mut links = world.resource_mut::<crate::LinkedRefOverrides>();
        links.set(MARKER, 0, NEXT);
        links.set(NEXT, 0, LAST);
        links.set(LAST, 0, MARKER); // a loop back to the start
        let mut enable = world.resource_mut::<crate::fragment::ReferenceEnableState>();
        enable.set_enabled(NEXT, false);
        enable.set_enabled(LAST, false);
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert!(crate::fragment::reference_enabled(&world, NEXT) && crate::fragment::reference_enabled(&world, LAST));
}

/// `Utility.RandomFloat(min, max)` stays within its bounds, and `GetScale()` reads
/// the reference's transform.
#[test]
fn random_float_and_scale() {
    let script = parsed_source(
        "ScriptName QF_VmFloat extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n\
           Float roll = Utility.RandomFloat(2.0, 4.0)\n\
           Float other = Utility.RandomFloat()\n\
           If roll >= 2.0 && roll <= 4.0 && other >= 0.0 && other <= 1.0 && Game.GetPlayer().GetScale() == 1.5\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.insert_resource(crate::story_manager::StoryManagerRng::default());
    let player = world.resource::<PapyrusPlayerEntity>().0;
    world.register::<byroredux_core::ecs::Transform>();
    world.insert(
        player,
        byroredux_core::ecs::Transform::new(
            byroredux_core::math::Vec3::ZERO,
            byroredux_core::math::Quat::IDENTITY,
            1.5,
        ),
    );
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// `InputEnableLayer`: `Create`, per-layer disables with the CK defaults, the
/// `Is…Enabled` queries, `Reset` and `Delete`; the player's controls are enabled
/// while no layer disables them.
#[test]
fn input_enable_layers_hold_and_release_controls() {
    let script = parsed_source(
        "ScriptName QF_VmLayer extends Quest\n\
         Function Fragment_1()\n\
           InputEnableLayer a = InputEnableLayer.Create()\n\
           InputEnableLayer b = InputEnableLayer.Create()\n\
           a.DisablePlayerControls()\n\
           b.EnableLooking(False)\n\
           If !a.IsMovementEnabled() && a.IsLookingEnabled() && !b.IsLookingEnabled() && b.IsMovementEnabled()\n\
             Self.SetStage(20)\n\
           EndIf\n\
           a.Delete()\n\
           b.Reset()\n\
           b.EnableMovement(False)\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.insert_resource(crate::player_control::PlayerControlState::default());
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
    // Layer a was deleted, b was reset and then disabled movement.
    let state = *world.resource::<crate::player_control::PlayerControlState>();
    assert!(!state.movement_enabled);
    assert!(state.fighting_enabled && state.looking_enabled && state.menu_enabled);
}

/// Help-message housekeeping statements run as no-ops (a help message is shown
/// once here), and only with the arguments they take.
#[test]
fn help_message_housekeeping_is_a_no_op() {
    let script = parsed_source(
        "ScriptName QF_VmHelp extends Quest\n\
         Message Property Help Auto\n\
         Function Fragment_1()\n\
           Message.ResetHelpMessage(\"Jump\")\n\
           Help.UnshowAsHelpMessage()\n\
           Message.ClearHelpMessages()\n\
           Self.SetStage(20)\n\
         EndFunction\n\
         Function Fragment_2()\n Message.ResetHelpMessage()\n EndFunction\n",
    );
    let message = parsed_source("ScriptName Message extends Form\nFunction ResetHelpMessage(String asEvent) Native Global\n");
    let scripts: HashMap<String, Arc<Script>> = [("message".to_owned(), message)].into();
    let library = Installed(&scripts);
    let needed = check_function_reason(Arc::clone(&script), "Fragment_1", Some(&library)).expect("runs");
    assert!(check_function_reason(Arc::clone(&script), "Fragment_2", Some(&library)).is_err());
    let world = world_with_fragment(script);
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        for needed in needed {
            fragments.insert_vm_script(needed);
        }
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// CK *IsChild - Location*: true when the other location is a child of this one,
/// false for the same location.
#[test]
fn location_is_child() {
    const PARENT: u32 = 0x0001_0F70;
    const CHILD: u32 = 0x0001_0F71;
    let script = parsed_source(
        "ScriptName QF_VmLoc extends Quest\n\
         Location Property ParentLoc Auto\n\
         Location Property ChildLoc Auto\n\
         Function Fragment_1()\n\
           If ParentLoc.IsChild(ChildLoc) && !ChildLoc.IsChild(ParentLoc) && !ParentLoc.IsChild(ParentLoc)\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    let object = |name: &str, form_id: u32| ScriptProperty {
        name: name.into(),
        status: 1,
        value: PropertyValue::Object { form_id, alias: -1 },
    };
    world.resource_mut::<crate::QuestStageFragments>().insert_vmad(
        Q,
        ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "QF_VmLoc".into(),
                status: 0,
                properties: vec![object("ParentLoc", PARENT), object("ChildLoc", CHILD)],
            }],
        },
    );
    let mut index = byroredux_plugin::esm::records::EsmIndex::default();
    for (form_id, parent) in [(PARENT, None), (CHILD, Some(PARENT))] {
        index.locations.insert(
            form_id,
            byroredux_plugin::esm::records::LctnRecord {
                form_id,
                parent,
                ..Default::default()
            },
        );
    }
    world.insert_resource(crate::location::LocationTable::from_index(&index));
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// A quest's `OnStageSet` reaches the scripts registered for it remotely, with
/// the sender first, for every stage set — a stage with no fragment included.
#[test]
fn quest_stage_set_is_a_remote_event() {
    const WATCHED: QuestFormId = QuestFormId(0x0001_0F80);
    let watcher = parsed_source(
        "ScriptName VmStageWatcher extends Quest\n\
         Quest Property Watched Auto\n\
         Int Property Seen Auto\n\
         Function Fragment_1()\n\
           RegisterForRemoteEvent(Watched as ScriptObject, \"OnStageSet\")\n\
         EndFunction\n\
         Event Quest_OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)\n\
           Seen = auiStageID\n\
         EndEvent\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&watcher), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(watcher);
    // The watched quest is a property of the watcher.
    world.resource_mut::<crate::QuestStageFragments>().insert_vmad(
        Q,
        ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "VmStageWatcher".into(),
                status: 0,
                properties: vec![ScriptProperty {
                    name: "Watched".into(),
                    status: 1,
                    value: PropertyValue::Object {
                        form_id: WATCHED.0,
                        alias: -1,
                    },
                }],
            }],
        },
    );
    world.insert_resource(crate::QuestScriptVariables::default());
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    // The registration is made; the watched quest advances (it has no fragment).
    world.resource_mut::<QuestStageState>().set_stage(WATCHED, 40);
    crate::fragment::quest_fragment_dispatch_system(&world);
    crate::fragment::fragment_continuation_system(&world, 0.0);
    let seen = world
        .resource::<crate::QuestScriptVariables>()
        .get(Q, crate::QuestScriptVariables::property_key("Seen"));
    assert_eq!(seen, Some(40.0));
}

/// An engine class's `AutoReadOnly` constant reads its value — on any
/// reference, and unqualified in a script that inherits it.
#[test]
fn engine_constants_read_their_values() {
    let object_reference = parsed_source(
        "ScriptName ObjectReference extends Form\n\
         Int Property Motion_Keyframed = 2 AutoReadOnly\n\
         Function Disable(Bool abFadeOut = False) Native\n",
    );
    let script = parsed_source(
        "ScriptName QF_VmConstants extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n\
           If Marker.Motion_Keyframed == 2\n Self.SetStage(20)\n EndIf\n\
         EndFunction\n",
    );
    let scripts: HashMap<String, Arc<Script>> = [("objectreference".to_owned(), object_reference)].into();
    let library = Installed(&scripts);
    let needed = check_function_reason(Arc::clone(&script), "Fragment_1", Some(&library)).expect("runs");
    let mut world = world_with_fragment(script);
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        for needed in needed {
            fragments.insert_vm_script(needed);
        }
    }
    world.insert_resource(crate::QuestScriptVariables::default());
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// A reference script's `OnDeath(akKiller)` runs from the victim's
/// `OnDeathEvent`, which its dispatcher drains (Pattern B).
#[test]
fn on_death_runs_with_the_killer() {
    const REF: u32 = 0x0001_0F12;
    let script = parsed_source(
        "ScriptName VmDeathScript extends Actor\n\
         ObjectReference Property Marker Auto\n\
         Event OnDeath(Actor akKiller)\n\
           If akKiller == Game.GetPlayer()\n Marker.Enable()\n EndIf\n\
         EndEvent\n",
    );
    let (events, scripts) =
        crate::check_reference_script(Arc::clone(&script), &crate::DELIVERED_EVENTS, None).expect("runs");
    assert_eq!(events, ["OnDeath"]);
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(REF),
    });
    world.insert_resource(pool);
    let victim = world.spawn();
    world.insert(victim, FormIdComponent(id));
    world
        .resource_mut::<crate::fragment::ReferenceEnableState>()
        .set_enabled(MARKER, false);
    let instance = ScriptInstance {
        name: "VmDeathScript".into(),
        status: 0,
        properties: vec![ScriptProperty {
            name: "Marker".into(),
            status: 1,
            value: PropertyValue::Object {
                form_id: MARKER,
                alias: -1,
            },
        }],
    };
    crate::attach_reference_script(&mut world, victim, &instance, events, scripts);
    world.insert(victim, crate::OnDeathEvent { killer: Some(player) });
    crate::reference_script_event_system(&world, 0.0);
    assert!(crate::fragment::reference_enabled(&world, MARKER));
    assert!(world.get::<crate::OnDeathEvent>(victim).is_none(), "drained by its consumer");
}

/// An interpreted topic-info fragment's `akSpeakerRef` is the line's speaker.
#[test]
fn info_fragments_bind_the_speaker() {
    let script = parsed_source(
        "ScriptName TIF__VmSpeaker extends TopicInfo\n\
         Function Fragment_0(ObjectReference akSpeakerRef)\n\
           Actor akSpeaker = akSpeakerRef as Actor\n\
           akSpeaker.Disable()\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_0", None).err(), None);
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(GUARD),
    });
    world.insert_resource(pool);
    let speaker = world.spawn();
    world.insert(speaker, FormIdComponent(id));
    world.resource_mut::<crate::QuestStageFragments>().insert_vm_script(script);
    let effect = Effect::Interpret {
        script: "TIF__VmSpeaker".into(),
        function: "Fragment_0".into(),
        args: Vec::new(),
        resume: None,
    };
    crate::apply_spoken_info_fragment(&world, std::slice::from_ref(&effect), Q, None, Some(speaker));
    assert!(!crate::fragment::reference_enabled(&world, GUARD));
}

/// A fragment calls a function of the script the VM runs on another
/// reference, reads its variable and writes another: the call runs as that
/// reference's (its `Self`, its variables), the writes land under it.
#[test]
fn calls_and_members_reach_scripts_on_other_references() {
    const TRAP: u32 = 0x0001_0F13;
    let trap_script = parsed_source(
        "ScriptName VmTrapBase extends ObjectReference\n\
         Int Property Hits Auto\n\
         Bool Armed = True\n\
         Function Fire()\n\
           If Armed\n Hits += 1\n Self.Disable()\n EndIf\n\
         EndFunction\n",
    );
    let script = parsed_source(
        "ScriptName QF_VmCrossRef extends Quest\n\
         ObjectReference Property Trap Auto\n\
         Function Fragment_1()\n\
           VmTrapBase t = Trap as VmTrapBase\n\
           t.Fire()\n\
           If t.Hits == 1\n (Trap as VmTrapBase).Armed = False\n Self.SetStage(20)\n EndIf\n\
         EndFunction\n",
    );
    let scripts: HashMap<String, Arc<Script>> = [("vmtrapbase".to_owned(), Arc::clone(&trap_script))].into();
    let library = Installed(&scripts);
    let needed = check_function_reason(Arc::clone(&script), "Fragment_1", Some(&library)).expect("runs");
    let mut world = world_with_fragment(script);
    world.insert_resource(crate::QuestScriptVariables::default());
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(TRAP),
    });
    world.insert_resource(pool);
    let trap = world.spawn();
    world.insert(trap, FormIdComponent(id));
    let instance = ScriptInstance {
        name: "VmTrapBase".into(),
        status: 0,
        properties: Vec::new(),
    };
    crate::attach_reference_script(&mut world, trap, &instance, Vec::new(), vec![trap_script]);
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        for needed in needed {
            fragments.insert_vm_script(needed);
        }
        // The fragment's `Trap` property.
        let mut vmad = vmad();
        vmad.scripts[0].properties.push(ScriptProperty {
            name: "Trap".into(),
            status: 1,
            value: PropertyValue::Object {
                form_id: TRAP,
                alias: -1,
            },
        });
        fragments.insert_vmad(Q, vmad);
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert!(!crate::fragment::reference_enabled(&world, TRAP), "Fire ran as the trap");
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20, "read the trap's Hits");
    let variable = |name: &str| {
        world
            .resource::<crate::QuestScriptVariables>()
            .get(QuestFormId(TRAP), crate::QuestScriptVariables::property_key(name))
    };
    assert_eq!(variable("Hits"), Some(1.0));
    assert_eq!(variable("Armed"), Some(0.0), "written under the trap");
}

/// `Game.IncrementStat` counts a misc stat; `Game.QueryStat` reads a
/// script-kept one, increments queued earlier in the run included.
#[test]
fn misc_stats_count_and_read() {
    let script = parsed_source(
        "ScriptName QF_VmStats extends Quest\n\
         Function Fragment_1()\n\
           Game.IncrementStat(\"Houses Owned\")\n\
           Game.IncrementStat(\"Houses Owned\", 2)\n\
           If Game.QueryStat(\"Houses Owned\") == 3\n Self.SetStage(20)\n EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let world = world_with_fragment(script);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
    assert_eq!(world.resource::<crate::MiscStats>().get("houses owned"), 3);
    // An engine-tracked stat is not counted here: the read declines.
    let days = parsed_source(
        "ScriptName QF_VmDays extends Quest\n\
         Function Fragment_1()\n If Game.QueryStat(\"Days Passed\") > 1\n Self.SetStage(20)\n EndIf\n EndFunction\n",
    );
    assert!(check_function(days, "Fragment_1", None).is_none());
}

/// `OnHit` receives the aggressor, the aggressor's equipped weapon as the
/// source, and the hit's flags.
#[test]
fn on_hit_receives_the_aggressor_weapon_and_flags() {
    const REF: u32 = 0x0001_0F14;
    const SWORD: u32 = 0x0001_2EB7;
    let script = parsed_source(
        "ScriptName VmHitScript extends ObjectReference\n\
         Weapon Property Sword Auto\n\
         Event OnHit(ObjectReference akAggressor, Form akSource, Projectile akProjectile, \
           Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked)\n\
           If akAggressor == Game.GetPlayer() && akSource == Sword && abPowerAttack && !abHitBlocked\n\
             Self.Disable()\n\
           EndIf\n\
         EndEvent\n",
    );
    let (events, scripts) =
        crate::check_reference_script(Arc::clone(&script), &crate::DELIVERED_EVENTS, None).expect("runs");
    assert_eq!(events, ["OnHit"]);
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    world.register::<FormIdComponent>();
    world.register::<byroredux_core::ecs::components::EquippedWeapon>();
    world.insert(
        player,
        byroredux_core::ecs::components::EquippedWeapon {
            inventory_index: byroredux_core::ecs::components::InventoryIndex(0),
            base_form_id: SWORD,
            damage: 7.0,
            reach: 0.0,
            speed: 0.0,
        },
    );
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(REF),
    });
    world.insert_resource(pool);
    let target = world.spawn();
    world.insert(target, FormIdComponent(id));
    let instance = ScriptInstance {
        name: "VmHitScript".into(),
        status: 0,
        properties: vec![ScriptProperty {
            name: "Sword".into(),
            status: 1,
            value: PropertyValue::Object {
                form_id: SWORD,
                alias: -1,
            },
        }],
    };
    crate::attach_reference_script(&mut world, target, &instance, events, scripts);
    world.insert(
        target,
        crate::HitEvent {
            aggressor: player,
            source: player,
            projectile: player,
            damage: 7.0,
            power_attack: true,
            sneak_attack: false,
            bash_attack: false,
            blocked: false,
            arrow: false,
            ignores_resistance: false,
        },
    );
    // #5991 — `OnHit` is delivered by the hit pass (scheduled after every
    // producer of the marker), not by the frame pass.
    crate::reference_script_event_system(&world, 0.0);
    assert!(crate::fragment::reference_enabled(&world, REF), "the frame pass leaves hits to the hit pass");
    crate::reference_script_hit_system(&world, 0.0);
    assert!(!crate::fragment::reference_enabled(&world, REF));
}

/// An engine constant argument passes to a native by name, so a per-game
/// number (`Motion_Keyframed` is 4 in Skyrim) never reaches the primitives
/// as a bare literal they cannot place.
#[test]
fn engine_constant_arguments_pass_by_name() {
    let object_reference = parsed_source(
        "ScriptName ObjectReference extends Form\n\
         Int Property Motion_Keyframed = 4 AutoReadOnly\n\
         Function SetMotionType(Int aiMotionType, Bool abAllowActivate = True) Native\n",
    );
    let script = parsed_source(
        "ScriptName QF_VmMotion extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n Marker.SetMotionType(Marker.Motion_Keyframed, True)\n EndFunction\n",
    );
    let scripts: HashMap<String, Arc<Script>> = [("objectreference".to_owned(), object_reference)].into();
    let library = Installed(&scripts);
    let needed = check_function_reason(Arc::clone(&script), "Fragment_1", Some(&library)).expect("runs");
    let mut world = world_with_fragment(script);
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        for needed in needed {
            fragments.insert_vm_script(needed);
        }
    }
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(MARKER),
    });
    world.insert_resource(pool);
    let marker = world.spawn();
    world.insert(marker, FormIdComponent(id));
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    let request = world
        .get::<crate::MotionTypeChangeRequest>(marker)
        .map(|request| request.motion_type);
    assert_eq!(request, Some(byroredux_core::ecs::components::MotionType::Keyframed));
}

/// A raw `SetMotionType` number is the game's: Skyrim's 4 is Keyframed (CK
/// *SetMotionType - ObjectReference*), and in another game it stops the run
/// instead of guessing.
#[test]
fn raw_motion_types_follow_the_game() {
    use byroredux_core::ecs::components::MotionType;
    use byroredux_plugin::esm::reader::GameKind;
    for (game, expected_stage, expected_motion) in [
        (GameKind::Skyrim, 20, Some(MotionType::Keyframed)),
        (GameKind::Fallout4, 10, None),
    ] {
        let script = parsed_source(
            "ScriptName QF_VmRawMotion extends Quest\n\
             ObjectReference Property Marker Auto\n\
             Function Fragment_1()\n\
               Marker.SetMotionType(4, True)\n\
               Self.SetStage(20)\n\
             EndFunction\n",
        );
        assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
        let mut world = world_with_fragment(script);
        world.insert_resource(crate::ScriptGame(game));
        world.register::<FormIdComponent>();
        let mut pool = FormIdPool::default();
        let id = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("Skyrim.esm"),
            local: LocalFormId(MARKER),
        });
        world.insert_resource(pool);
        let marker = world.spawn();
        world.insert(marker, FormIdComponent(id));
        world.resource_mut::<QuestStageState>().set_stage(Q, 10);
        crate::fragment::quest_fragment_dispatch_system(&world);
        assert_eq!(world.resource::<QuestStageState>().get_stage(Q), expected_stage, "{game:?}");
        assert_eq!(
            world.get::<crate::MotionTypeChangeRequest>(marker).map(|request| request.motion_type),
            expected_motion,
            "{game:?}"
        );
    }
}

/// Fallout 4 timers: `StartTimer(afInterval, aiTimerID)` fires `OnTimer` once
/// with its ID; `CancelTimer` stops one; a game-time interval under 0.033
/// hours rounds up to it.
#[test]
fn timers_fire_once_with_their_id() {
    let script = parsed_source(
        "ScriptName QF_VmTimers extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Int Fired\n\
         Function Fragment_1()\n\
           StartTimer(5.0, 7)\n\
           StartTimer(1.0, 8)\n\
           CancelTimer(8)\n\
           StartTimerGameTime(0.01, 3)\n\
         EndFunction\n\
         Event OnTimer(Int aiTimerID)\n\
           If aiTimerID == 7\n Marker.Enable()\n EndIf\n\
           Fired += 1\n\
         EndEvent\n\
         Event OnTimerGameTime(Int aiTimerID)\n\
           If aiTimerID == 3\n Self.SetStage(20)\n EndIf\n\
         EndEvent\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.insert_resource(crate::QuestScriptVariables::default());
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    let fired = |world: &World| {
        world
            .resource::<crate::QuestScriptVariables>()
            .get(Q, crate::QuestScriptVariables::property_key("Fired"))
    };
    crate::script_update_system(&world, 2.0, Some(0.0));
    assert_eq!(fired(&world), None, "timer 8 was cancelled, 7 is not due");
    crate::script_update_system(&world, 3.5, Some(0.02));
    assert!(crate::fragment::reference_enabled(&world, MARKER), "OnTimer(7)");
    assert_eq!(fired(&world), Some(1.0));
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 10, "0.02 h < the 0.033 h floor");
    crate::script_update_system(&world, 0.1, Some(0.04));
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
    crate::script_update_system(&world, 10.0, Some(1.0));
    assert_eq!(fired(&world), Some(1.0), "a timer fires once");
}

/// Another quest's member reads by its declared type — here an object
/// script variable the other quest's script holds — not as a number.
#[test]
fn another_quests_members_read_by_their_declared_type() {
    const OTHER: QuestFormId = QuestFormId(0x0001_0F20);
    let other_script = parsed_source(
        "ScriptName VmOtherScript extends Quest\n\
         ObjectReference Stored\n",
    );
    let script = parsed_source(
        "ScriptName QF_VmQuestMembers extends Quest\n\
         Quest Property Other Auto\n\
         Function Fragment_1()\n\
           VmOtherScript o = Other as VmOtherScript\n\
           o.Stored.Disable()\n\
         EndFunction\n",
    );
    let scripts: HashMap<String, Arc<Script>> = [("vmotherscript".to_owned(), Arc::clone(&other_script))].into();
    let library = Installed(&scripts);
    let needed = check_function_reason(Arc::clone(&script), "Fragment_1", Some(&library)).expect("runs");
    let mut world = world_with_fragment(script);
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(GUARD),
    });
    world.insert_resource(pool);
    let guard = world.spawn();
    world.insert(guard, FormIdComponent(id));
    let other_vmad = ScriptInstanceData {
        version: 5,
        object_format: 2,
        scripts: vec![ScriptInstance {
            name: "VmOtherScript".into(),
            status: 0,
            properties: Vec::new(),
        }],
    };
    world.insert_resource(crate::QuestScriptInstances(Arc::new([(OTHER, other_vmad)].into())));
    world.resource_mut::<crate::QuestScriptObjects>().set(
        OTHER,
        crate::QuestScriptVariables::property_key("Stored"),
        crate::ScriptObject::Form(GUARD),
    );
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        for needed in needed {
            fragments.insert_vm_script(needed);
        }
        let mut vmad = vmad();
        vmad.scripts[0].properties.push(ScriptProperty {
            name: "Other".into(),
            status: 1,
            value: PropertyValue::Object {
                form_id: OTHER.0,
                alias: -1,
            },
        });
        fragments.insert_vmad(Q, vmad);
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert!(!crate::fragment::reference_enabled(&world, GUARD));
}

/// `ScriptName.Fn()` calls a global function of another script (no `Self`);
/// `Game.GetFormFromFile` resolves a plugin-local id through the load order,
/// its top byte unused. The load order registers its plugins under their
/// case-folded basenames, so the script's `"Skyrim.esm"` must find the
/// identity registered as `skyrim.esm` (LMC-ID-001: before the fold the two
/// hashed apart and the call returned None).
#[test]
fn global_functions_and_forms_from_files() {
    let util = parsed_source(
        "ScriptName VmUtil\n\
         Int Function Twice(Int n) Global\n Return n * 2\n EndFunction\n\
         Function DisableIt(ObjectReference akRef) Global\n akRef.Disable()\n EndFunction\n",
    );
    let script = parsed_source(
        "ScriptName QF_VmGlobals2 extends Quest\n\
         Function Fragment_1()\n\
           If VmUtil.Twice(3) == 6\n\
             VmUtil.DisableIt(Game.GetFormFromFile(0x41010F01, \"Skyrim.esm\") as ObjectReference)\n\
           EndIf\n\
         EndFunction\n",
    );
    let scripts: HashMap<String, Arc<Script>> = [("vmutil".to_owned(), Arc::clone(&util))].into();
    let library = Installed(&scripts);
    let needed = check_function_reason(Arc::clone(&script), "Fragment_1", Some(&library)).expect("runs");
    let mut world = world_with_fragment(script);
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        for needed in needed {
            fragments.insert_vm_script(needed);
        }
    }
    world.insert_resource(crate::load_order::LoadOrderIdentity::new(vec![(
        byroredux_plugin::esm::reader::GlobalSlot::Regular(0),
        PluginId::from_filename("skyrim.esm"),
    )]));
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("skyrim.esm"),
        local: LocalFormId(GUARD),
    });
    world.insert_resource(pool);
    let guard = world.spawn();
    world.insert(guard, FormIdComponent(id));
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert!(!crate::fragment::reference_enabled(&world, GUARD));
}

/// Papyrus `Int` arithmetic and comparison are exact 32-bit — beyond the 24
/// bits an `f32` holds.
#[test]
fn int_arithmetic_is_exact() {
    let add = arithmetic(BinaryOp::Add, &Value::Int(0x4101_0F00), &Value::Int(1)).unwrap();
    assert!(matches!(add, Value::Int(0x4101_0F01)));
    assert!(!Value::Int(0x4101_0F01).equals(&Value::Int(0x4101_0F00)));
    let lt = binary(BinaryOp::Lt, &Value::Int(0x4101_0F00), &Value::Int(0x4101_0F01)).unwrap();
    assert!(matches!(lt, Value::Bool(true)));
}

/// Fallout 4 structs and array methods: `new Struct`, members read and
/// written, `Add`, `FindStruct`, `Remove`, `Length`, an array-of-struct
/// property from the VMAD.
#[test]
fn structs_and_array_methods_run() {
    let script = parsed_source(
        "ScriptName QF_VmStructs extends Quest\n\
         Struct Entry\n Int Id\n ObjectReference Target\n EndStruct\n\
         ObjectReference Property Marker Auto\n\
         Entry[] Property Rows Auto\n\
         Function Fragment_1()\n\
           Entry[] entries = new Entry[0]\n\
           Entry first = new Entry\n\
           first.Id = 3\n\
           first.Target = Marker\n\
           entries.Add(first)\n\
           Entry second = new Entry\n\
           second.Id = 7\n\
           entries.Add(second)\n\
           Int i = entries.FindStruct(\"Id\", 3)\n\
           If i == 0 && entries.FindStruct(\"Id\", 9) < 0 && entries.Length == 2\n\
             entries[i].Target.Enable()\n\
             entries.Remove(0)\n\
             If entries.Length == 1 && entries[0].Id == 7 && Rows[0].Id == 11\n\
               Self.SetStage(20)\n\
             EndIf\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let world = world_with_fragment(script);
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        let mut vmad = vmad();
        vmad.scripts[0].properties.push(ScriptProperty {
            name: "Rows".into(),
            status: 1,
            value: PropertyValue::Array(vec![PropertyValue::Struct(vec![
                byroredux_plugin::esm::records::script_instance::StructMember {
                    name: "Id".into(),
                    flags: 1,
                    value: PropertyValue::Int32(11),
                },
            ])]),
        });
        fragments.insert_vmad(Q, vmad);
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert!(crate::fragment::reference_enabled(&world, MARKER), "entries[0].Target.Enable()");
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// A package's Begin / Change fragments run with `akActor` bound to the
/// actor; a switch of package runs the old one's Change and the new one's
/// Begin; a completed procedure runs the package's End fragment.
#[test]
fn package_fragments_run_on_transitions() {
    use byroredux_plugin::esm::records::script_instance::PackageFragmentEvent;
    const PACK_A: u32 = 0x0001_0F30;
    const PACK_B: u32 = 0x0001_0F31;
    const ACTOR: u32 = 0x0001_0F32;
    let a = parsed_source(
        "ScriptName PF_A extends Package\n\
         Function Fragment_0(Actor akActor)\n akActor.Disable()\n EndFunction\n\
         Function Fragment_1(Actor akActor)\n Self.SetStage(30)\n EndFunction\n\
         Function Fragment_2(Actor akActor)\n akActor.Disable()\n EndFunction\n",
    );
    let b = parsed_source(
        "ScriptName PF_B extends Package\n\
         Function Fragment_0(Actor akActor)\n akActor.Enable()\n EndFunction\n",
    );
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(ACTOR),
    });
    world.insert_resource(pool);
    let actor = world.spawn();
    world.insert(actor, FormIdComponent(id));
    let installed = std::cell::RefCell::new(Vec::new());
    let library: HashMap<String, Arc<Script>> = HashMap::new();
    let library = Installed(&library);
    for (package, script, bindings) in [
        (
            PACK_A,
            a,
            vec![
                (PackageFragmentEvent::Begin, "Fragment_0"),
                (PackageFragmentEvent::Change, "Fragment_1"),
                (PackageFragmentEvent::End, "Fragment_2"),
            ],
        ),
        (PACK_B, b, vec![(PackageFragmentEvent::Begin, "Fragment_0")]),
    ] {
        let mut fragments = world.resource_mut::<crate::PackageFragments>();
        let n = crate::populate_package_fragments(&mut fragments, package, None, script, &bindings, &library, &mut |s| {
            installed.borrow_mut().push(s)
        });
        assert_eq!(n, bindings.len());
    }
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        for script in installed.into_inner() {
            fragments.insert_vm_script(script);
        }
    }
    crate::report_package_transition(&world, actor, None, Some(PACK_A));
    crate::package_fragment_system(&world, 0.0);
    assert!(!crate::fragment::reference_enabled(&world, ACTOR), "A began: akActor disabled");
    crate::report_package_transition(&world, actor, Some(PACK_A), Some(PACK_B));
    crate::package_fragment_system(&world, 0.0);
    assert!(crate::fragment::reference_enabled(&world, ACTOR), "B began: akActor enabled");
    // B has no End binding: its completion runs nothing.
    crate::report_package_end(&world, actor, PACK_B);
    crate::package_fragment_system(&world, 0.0);
    assert!(crate::fragment::reference_enabled(&world, ACTOR), "B has no End fragment");
    // A completion of A runs its End fragment.
    crate::report_package_end(&world, actor, PACK_A);
    crate::package_fragment_system(&world, 0.0);
    assert!(!crate::fragment::reference_enabled(&world, ACTOR), "A ended: akActor disabled");
}

/// `GetBaseActorValue` reads the base without the modifiers, and
/// `GetActorValuePercentage` the current value over the undamaged ceiling
/// (CK: "as a percentage of its maximum value (from 0 to 1)").
#[test]
fn base_and_percentage_actor_value_reads() {
    use byroredux_core::ecs::components::ActorValues;
    use byroredux_plugin::esm::records::AvifRecord;
    const AV: u32 = 0x0000_05F0;
    let script = parsed_source(
        "ScriptName QF_VmAv extends Quest\n\
         Function Fragment_1()\n\
           Actor player = Game.GetPlayer()\n\
           Float base = player.GetBaseActorValue(\"Variable06\")\n\
           Float fraction = player.GetActorValuePercentage(\"Variable06\")\n\
           If base == 10.0 && fraction == 0.5 && player.GetActorValue(\"Variable06\") == 10.0\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.register::<ActorValues>();
    let player = world.resource::<PapyrusPlayerEntity>().0;
    let mut index = byroredux_plugin::esm::records::EsmIndex::default();
    index.actor_values.insert(
        AV,
        AvifRecord { form_id: AV, editor_id: "AVVariable06".into(), ..Default::default() },
    );
    crate::install_actor_value_names(&mut world, &index);
    // base 10, +10 permanent, 10 damage: current 10 of an undamaged 20.
    let mut values = ActorValues::new();
    values.set_base(AV, 10.0);
    values.mod_permanent(AV, 10.0);
    values.apply_damage(AV, 10.0);
    world.insert(player, values);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// `<MusicType>.Add()` / `.Remove()` are accepted (there is no music player to
/// drive), and the statements after them run; a receiver that is not declared a
/// `MusicType` is not one.
#[test]
fn music_type_add_and_remove_are_accepted_only_on_a_music_type() {
    let accepted = parsed_source(
        "ScriptName QF_VmMusic extends Quest\n\
         MusicType Property Theme Auto\n\
         Function Fragment_1()\n\
           Theme.Add()\n\
           Theme.Remove()\n\
           Self.SetStage(20)\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&accepted), "Fragment_1", None).err(), None);
    let world = world_with_fragment(accepted);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);

    let other = parsed_source(
        "ScriptName QF_VmMusicNot extends Quest\n\
         Quest Property Other Auto\n\
         Function Fragment_1()\n\
           Other.Add()\n\
         EndFunction\n",
    );
    assert!(check_function_reason(other, "Fragment_1", None).is_err());
}

/// A helper that forwards its own parameters to `ShowAsHelpMessage` (the
/// Starfield shape): the check treats the run-time context as the unrestricted
/// one, and a call with an empty context runs through.
#[test]
fn help_message_helper_with_run_time_arguments_runs() {
    let script = parsed_source(
        "ScriptName QF_VmHelp extends Quest\n\
         Message Property Marker Auto\n\
         Function Help(String asEvent, Float afDuration, String asContext)\n\
           Marker.ShowAsHelpMessage(asEvent, afDuration, 30.0, 3, asContext, 0, None)\n\
         EndFunction\n\
         Function Fragment_1()\n\
           Help(\"Jump\", 5.0, \"\")\n\
           Self.SetStage(20)\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let world = world_with_fragment(script);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// `GetCurrentPackage` reads the package the AI last reported for the actor,
/// and None once it reports none.
#[test]
fn get_current_package_follows_the_reported_transitions() {
    const PACKAGE: u32 = 0x0001_0F40;
    let script = parsed_source(
        "ScriptName QF_VmPackage extends Quest\n\
         Actor Property Marker Auto\n\
         Function Fragment_1()\n\
           If Marker.GetCurrentPackage()\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(MARKER),
    });
    world.insert_resource(pool);
    let marker = world.spawn();
    world.insert(marker, FormIdComponent(id));
    let run = |world: &World| {
        world.resource_mut::<QuestStageState>().set_stage(Q, 10);
        crate::fragment::quest_fragment_dispatch_system(world);
        world.resource::<QuestStageState>().get_stage(Q)
    };
    assert_eq!(run(&world), 10, "no package reported");
    crate::report_package_transition(&world, marker, None, Some(PACKAGE));
    assert_eq!(world.resource::<crate::ActivePackages>().0.get(&marker), Some(&PACKAGE));
    assert_eq!(run(&world), 20, "a package is running");
    crate::report_package_transition(&world, marker, Some(PACKAGE), None);
    assert_eq!(run(&world), 10, "the package ended");
}

/// `GetFactionReaction` reports 0 Neutral, 1 Enemy, 2 Ally, 3 Friend from the
/// two actors' faction memberships and the scripted relations.
#[test]
fn get_faction_reaction_reads_the_factions() {
    use byroredux_core::ecs::components::FactionRanks;
    use byroredux_sdk::relationships::CombatReaction;
    const GUARDS: u32 = 0x0000_0123;
    const BANDITS: u32 = 0x0000_0124;
    let script = parsed_source(
        "ScriptName QF_VmReaction extends Quest\n\
         Actor Property Marker Auto\n\
         Function Fragment_1()\n\
           If Marker.GetFactionReaction(Marker) == 1\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.register::<FormIdComponent>();
    world.register::<FactionRanks>();
    let identity = crate::LoadOrderIdentity::new(vec![(
        byroredux_plugin::esm::reader::GlobalSlot::Regular(0),
        PluginId::from_filename("Skyrim.esm"),
    )]);
    let guards = identity.form_ref(GUARDS).unwrap();
    let bandits = identity.form_ref(BANDITS).unwrap();
    world.insert_resource(identity);
    let mut pool = FormIdPool::default();
    let marker_id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(MARKER) });
    world.insert_resource(pool);
    // One actor in both factions: the guards' view of the bandits is its own.
    let marker = world.spawn();
    world.insert(marker, FormIdComponent(marker_id));
    world.insert(marker, FactionRanks(vec![(GUARDS, 0), (BANDITS, 0)]));
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 10, "neutral by default");
    world.resource_mut::<crate::FactionRelations>().set_reaction(guards, bandits, CombatReaction::Enemy);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20, "enemy once scripted");
}

/// A terminal menu pick runs the picked item's fragment with `akTerminalRef`
/// bound to the terminal reference; another item's fragment does not run.
#[test]
fn terminal_selection_runs_the_items_fragment() {
    const TERMINAL: u32 = 0x0001_0F50;
    let script = parsed_source(
        "ScriptName TERM_VmTest extends Terminal\n\
         Function Fragment_Terminal_01(ObjectReference akTerminalRef)\n\
           akTerminalRef.Disable()\n\
         EndFunction\n\
         Function Fragment_Terminal_02(ObjectReference akTerminalRef)\n\
           akTerminalRef.Enable()\n\
         EndFunction\n",
    );
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    let installed = std::cell::RefCell::new(Vec::new());
    let library: HashMap<String, Arc<Script>> = HashMap::new();
    let library = Installed(&library);
    {
        let mut fragments = world.resource_mut::<crate::TerminalFragments>();
        let n = crate::populate_fragments(
            &mut fragments,
            TERMINAL,
            None,
            script,
            &[(crate::TerminalItem(1), "Fragment_Terminal_01"), (crate::TerminalItem(2), "Fragment_Terminal_02")],
            &library,
            &mut |s| installed.borrow_mut().push(s),
        );
        assert_eq!(n, 2);
    }
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        for script in installed.into_inner() {
            fragments.insert_vm_script(script);
        }
    }
    // The terminal reference is the marker the tests already track.
    crate::report_terminal_selection(&world, MARKER, TERMINAL, 1);
    crate::terminal_fragment_system(&world, 0.0);
    assert!(!crate::fragment::reference_enabled(&world, MARKER), "item 1 disabled the terminal");
    crate::report_terminal_selection(&world, MARKER, TERMINAL, 2);
    crate::terminal_fragment_system(&world, 0.0);
    assert!(crate::fragment::reference_enabled(&world, MARKER), "item 2 enabled it again");
    crate::report_terminal_selection(&world, MARKER, TERMINAL, 3);
    crate::terminal_fragment_system(&world, 0.0);
    assert!(crate::fragment::reference_enabled(&world, MARKER), "no fragment for item 3");
}

/// `DamageActorValue` grows the damage layer and `RestoreActorValue` takes it
/// back, never past zero (CK *DamageValue* / *RestoreValue*).
#[test]
fn damage_and_restore_actor_value_move_the_damage_layer() {
    use byroredux_core::ecs::components::ActorValues;
    use byroredux_plugin::esm::records::AvifRecord;
    const AV: u32 = 0x0000_05F0;
    let script = parsed_source(
        "ScriptName QF_VmDamage extends Quest\n\
         Function Fragment_1()\n\
           Actor player = Game.GetPlayer()\n\
           Float amount = 4.0\n\
           player.DamageActorValue(\"Variable06\", amount)\n\
           player.DamageAV(\"Variable06\", 3.0)\n\
           player.RestoreActorValue(\"Variable06\", 5.0)\n\
           If player.GetActorValue(\"Variable06\") == 8.0\n\
             player.RestoreAV(\"Variable06\", 99.0)\n\
             If player.GetActorValue(\"Variable06\") == 10.0\n\
               Self.SetStage(20)\n\
             EndIf\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.register::<ActorValues>();
    let player = world.resource::<PapyrusPlayerEntity>().0;
    let mut index = byroredux_plugin::esm::records::EsmIndex::default();
    index.actor_values.insert(
        AV,
        AvifRecord { form_id: AV, editor_id: "AVVariable06".into(), ..Default::default() },
    );
    crate::install_actor_value_names(&mut world, &index);
    let mut values = ActorValues::new();
    values.set_base(AV, 10.0);
    world.insert(player, values);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// #5862 — a script taking Health to zero (`DamageActorValue`, `SetActorValue`)
/// owes the same death edge a scripted `Kill` has: the actor is queued for
/// `scripted_kill_system` (spare / `Dead` / KILL / teardown). Above zero,
/// restores and non-Health values queue nothing.
#[test]
fn script_health_writes_queue_the_death_edge() {
    use byroredux_core::ecs::components::{ActorValues, ActorVitals};
    use byroredux_plugin::esm::records::AvifRecord;
    const HEALTH: u32 = 0x0000_02D4;
    const OTHER: u32 = 0x0000_05F0;
    let script = parsed_source(
        "ScriptName QF_VmHealthDeath extends Quest\n\
         Function Fragment_1()\n\
           Actor player = Game.GetPlayer()\n\
           player.DamageActorValue(\"Variable06\", 99.0)\n\
           player.DamageActorValue(\"Health\", 10.0)\n\
           player.RestoreActorValue(\"Health\", 5.0)\n\
         EndFunction\n\
         Function Fragment_2()\n\
           Actor player = Game.GetPlayer()\n\
           player.SetActorValue(\"Health\", 0.0)\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_2", None).err(), None);
    let mut world = world_with_fragment(script);
    world.register::<ActorValues>();
    world.register::<ActorVitals>();
    world.init_resource::<crate::story_manager::PendingScriptKills>();
    let player = world.resource::<PapyrusPlayerEntity>().0;
    let mut index = byroredux_plugin::esm::records::EsmIndex::default();
    for (form_id, name) in [(HEALTH, "AVHealth"), (OTHER, "AVVariable06")] {
        index.actor_values.insert(
            form_id,
            AvifRecord { form_id, editor_id: name.into(), ..Default::default() },
        );
    }
    crate::install_actor_value_names(&mut world, &index);
    let mut values = ActorValues::new();
    values.set_base(HEALTH, 10.0);
    values.set_base(OTHER, 10.0);
    world.insert(player, values);
    world.insert(player, ActorVitals { health: HEALTH });
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    // Health 10 − 10 (+5 restore) is 5 in the end, but the damage step hit 0
    // in between: the edge is queued the moment Health reaches zero.
    let kills = world.resource::<crate::story_manager::PendingScriptKills>().0.clone();
    assert_eq!(kills, vec![(player, None, false)], "queued once, for the Health write only");
}

/// `Math` natives, `Utility.GetCurrentGameTime` (days from the host's game
/// clock) and `Game.AddAchievement` (recorded once).
#[test]
fn math_game_time_and_achievements() {
    let script = parsed_source(
        "ScriptName QF_VmMath extends Quest\n\
         Function Fragment_1()\n\
           If Math.Floor(Math.Min(7.9, 12.0)) == 7 && Math.Ceiling(2.1) == 3 && Math.Pow(2.0, 8.0) == 256.0\n\
             If Math.Abs(-5.0) == 5.0 && Utility.GetCurrentGameTime() > 1.4\n\
               Game.AddAchievement(9)\n\
               Game.AddAchievement(9)\n\
               Self.SetStage(20)\n\
             EndIf\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let world = world_with_fragment(script);
    // 36 game hours = day 1.5.
    crate::script_update_system(&world, 0.0, Some(36.0));
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
    assert!(world.resource::<crate::Achievements>().has(9));
}

/// Starfield's `SetLinkedRef(apRef, apKeyword, abPromoteParentRefr)` link
/// lands in the saved overrides, and `None` clears it again.
#[test]
fn set_linked_ref_writes_the_saved_link() {
    let script = parsed_source(
        "ScriptName QF_VmLink extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n\
           Int count = 0\n\
           While count < 2\n\
             count += 1\n\
           EndWhile\n\
           Marker.SetLinkedRef(Marker, None, True)\n\
           Self.SetStage(20)\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let world = world_with_fragment(script);
    assert_eq!(crate::linked_ref(&world, MARKER, 0), None);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
    assert_eq!(crate::linked_ref(&world, MARKER, 0), Some(MARKER));
}

/// `GetNthLinkedRef(n, keyword)` follows the link under the keyword n times:
/// 0 is the reference itself, and a chain that ends reads `None`.
#[test]
fn get_nth_linked_ref_follows_the_chain_under_a_keyword() {
    const SECOND: u32 = 0x0002_0001;
    const THIRD: u32 = 0x0002_0002;
    let script = parsed_source(
        "ScriptName QF_VmNth extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n\
           If Marker.GetNthLinkedRef(0, None) == Marker && Marker.GetNthLinkedRef(2, None) && !Marker.GetNthLinkedRef(3, None)\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let world = world_with_fragment(script);
    {
        let mut links = world.resource_mut::<crate::LinkedRefOverrides>();
        links.set(MARKER, 0, SECOND);
        links.set(SECOND, 0, THIRD);
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// A form list's script edits: `AddForm` appends, `RemoveAddedForm` takes added
/// forms back, `Revert` drops them all; the run sees its own edits and they
/// reach the saved additions.
#[test]
fn form_list_edits_are_seen_by_the_run_and_kept() {
    let script = parsed_source(
        "ScriptName QF_VmFormList extends Quest\n\
         FormList Property Marker Auto\n\
         Function Fragment_1()\n\
           Marker.AddForm(Marker)\n\
           If Marker.GetSize() == 3 && Marker.HasForm(Marker)\n\
             Marker.RemoveAddedForm(Marker)\n\
             If Marker.GetSize() == 2\n\
               Marker.AddForm(Marker)\n\
               Self.SetStage(20)\n\
             EndIf\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    let mut index = byroredux_plugin::esm::records::EsmIndex::default();
    index.form_lists.insert(
        MARKER,
        byroredux_plugin::esm::records::FlstRecord { form_id: MARKER, entries: vec![0xA, 0xB], ..Default::default() },
    );
    crate::install_form_lists(&mut world, &index);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
    assert_eq!(world.resource::<crate::FormListAdditions>().of(MARKER), &[MARKER], "the kept edit");
    assert_eq!(crate::forms_of(&world, MARKER, &[]), Some(vec![0xA, 0xB, MARKER]));
}

/// `Game.GetCaps()` is Fallout 4's caps item, and stops the run in other games.
#[test]
fn get_caps_is_fallout_4s_caps_item() {
    use byroredux_plugin::esm::reader::GameKind;
    for (game, expected_stage) in [(GameKind::Fallout4, 20), (GameKind::Skyrim, 10)] {
        let script = parsed_source(
            "ScriptName QF_VmCaps extends Quest\n\
             Function Fragment_1()\n\
               Form caps = Game.GetCaps() as Form\n\
               If caps\n\
                 Self.SetStage(20)\n\
               EndIf\n\
             EndFunction\n",
        );
        assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
        let mut world = world_with_fragment(script);
        world.insert_resource(crate::ScriptGame(game));
        world.resource_mut::<QuestStageState>().set_stage(Q, 10);
        crate::fragment::quest_fragment_dispatch_system(&world);
        assert_eq!(world.resource::<QuestStageState>().get_stage(Q), expected_stage, "{game:?}");
    }
}

/// `Game.GetCredits()` is Starfield's currency form (the corpus passes it as the
/// `RemoveItem` item), and stops the run in other games.
#[test]
fn get_credits_is_starfields_currency_form() {
    use byroredux_plugin::esm::reader::GameKind;
    for (game, expected_stage) in [(GameKind::Starfield, 20), (GameKind::Fallout4, 10)] {
        let script = parsed_source(
            "ScriptName QF_VmCredits extends Quest\n\
             Function Fragment_1()\n\
               Form credits = Game.GetCredits() as Form\n\
               If credits\n\
                 Self.SetStage(20)\n\
               EndIf\n\
             EndFunction\n",
        );
        assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
        let mut world = world_with_fragment(script);
        world.insert_resource(crate::ScriptGame(game));
        world.resource_mut::<QuestStageState>().set_stage(Q, 10);
        crate::fragment::quest_fragment_dispatch_system(&world);
        assert_eq!(world.resource::<QuestStageState>().get_stage(Q), expected_stage, "{game:?}");
    }
}

/// The animation-graph natives answer with the CK's failure results while no
/// behavior graph runs, and the script around them still runs to its end.
#[test]
fn animation_graph_natives_report_no_graph() {
    let script = parsed_source(
        "ScriptName QF_VmAnim extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n\
           If !Marker.PlayAnimation(\"flip\") && !Marker.RegisterForAnimationEvent(Marker, \"reset\")\n\
             Marker.PlayAnimationAndWait(\"flip\", \"done\")\n\
             Marker.SetAnimationVariableFloat(\"Speed\", 1.0)\n\
             Marker.SetAnimationVariableBool(\"bOpen\", True)\n\
             Marker.UnregisterForAnimationEvent(Marker, \"reset\")\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let world = world_with_fragment(script);
    world.resource_mut::<crate::QuestStageFragments>().insert_vmad(
        Q,
        ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "QF_VmAnim".into(),
                status: 0,
                properties: vec![ScriptProperty {
                    name: "Marker".into(),
                    status: 1,
                    value: PropertyValue::Object { form_id: MARKER, alias: -1 },
                }],
            }],
        },
    );
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// `SetAnimationVariable*` writes the reference's behavior-graph variable and
/// `GetAnimationVariable*` reads it back (CK: "the value of a variable on the
/// reference's animation graph"); an int keeps whole numbers and a bool is a flag.
#[test]
fn animation_variables_are_written_and_read_on_the_graph() {
    use byroredux_hkx::{BehaviorGraph, State, StateMachine};
    const OTHER: u32 = 0x0001_0F91;
    let script = parsed_source(
        "ScriptName QF_VmAnimVars extends Quest\n\
         ObjectReference Property Marker Auto\n\
         ObjectReference Property Bare Auto\n\
         Float SeenSpeed\n\
         Int SeenMode\n\
         Int FlagSeen\n\
         Float BareSeen\n\
         Function Fragment_1()\n\
           Marker.SetAnimationVariableFloat(\"Speed\", 3.5)\n\
           Marker.SetAnimationVariableInt(\"iMode\", 7)\n\
           Marker.SetAnimationVariableBool(\"bFlag\", True)\n\
           SeenSpeed = Marker.GetAnimationVariableFloat(\"Speed\")\n\
           SeenMode = Marker.GetAnimationVariableInt(\"iMode\")\n\
           If Marker.GetAnimationVariableBool(\"bFlag\")\n\
             FlagSeen = 1\n\
           EndIf\n\
           Bare.SetAnimationVariableFloat(\"Speed\", 9.0)\n\
           BareSeen = Bare.GetAnimationVariableFloat(\"Speed\")\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.insert_resource(crate::QuestScriptVariables::default());
    world.register::<FormIdComponent>();
    world.register::<crate::ObjectAnimationGraph>();
    let mut pool = FormIdPool::default();
    let marker_id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(MARKER) });
    let bare_id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(OTHER) });
    world.insert_resource(pool);
    let marker = world.spawn();
    world.insert(marker, FormIdComponent(marker_id));
    let bare = world.spawn();
    world.insert(bare, FormIdComponent(bare_id));
    let variable = |name: &str, kind, initial| byroredux_hkx::Variable { name: name.into(), kind, initial };
    let graph = BehaviorGraph {
        variables: vec![
            variable("Speed", byroredux_hkx::VariableKind::Real, 0.0),
            variable("iMode", byroredux_hkx::VariableKind::Int, 0.0),
            variable("bFlag", byroredux_hkx::VariableKind::Bool, 0.0),
        ],
        events: Vec::new(),
        machine: Some(StateMachine {
            start_state: 0,
            start_state_variable: None,
            states: vec![State { id: 0, name: "Idle".into(), transitions: Vec::new(), clip: None, content: Default::default() }],
            wildcard: Vec::new(),
        }),
    };
    world.insert(
        marker,
        crate::ObjectAnimationGraph::new(
            Arc::new(crate::LoadedGraph { graph, clips: Default::default(), sequences: Default::default(), lazy: None }),
            marker,
        )
        .unwrap(),
    );
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        let mut vmad = vmad();
        for (name, form_id) in [("Marker", MARKER), ("Bare", OTHER)] {
            vmad.scripts[0].properties.push(ScriptProperty {
                name: name.into(),
                status: 1,
                value: PropertyValue::Object { form_id, alias: -1 },
            });
        }
        fragments.insert_vmad(Q, vmad);
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    let seen = |name: &str| {
        world.resource::<crate::QuestScriptVariables>().get(Q, crate::QuestScriptVariables::property_key(name))
    };
    assert_eq!(seen("SeenSpeed"), Some(3.5));
    assert_eq!(seen("SeenMode"), Some(7.0));
    assert_eq!(seen("FlagSeen"), Some(1.0));
    assert_eq!(seen("BareSeen"), Some(0.0), "a reference with no graph reads zero and takes the write for nothing");
    assert_eq!(crate::animation_graph::variable_value(&world, marker, "speed"), Some(3.5));
    assert_eq!(crate::animation_graph::variable_value(&world, bare, "Speed"), None);
}

/// `Actor.ModFavorPoints(n)` is, in Skyrim.esm's `Actor.pex`, `If n > 0: MakePlayerFriend()`,
/// and `MakePlayerFriend()` (CK) makes a *unique* actor whose relationship rank with the
/// player is 0 a rank-1 friend; any other call changes nothing.
#[test]
fn mod_favor_points_befriends_a_unique_actor_with_a_positive_amount() {
    use byroredux_plugin::esm::records::{EsmIndex, NpcRecord};
    const FRIEND: u32 = 0x0001_0F91;
    const FOE: u32 = 0x0001_0F92;
    const OTHER_UNIQUE: u32 = 0x0001_0F93;
    const FAVOR_GLOBAL: u32 = 0x0001_0F94;
    const UNIQUE_NPC: u32 = 0x0002_0001;
    const COMMON_NPC: u32 = 0x0002_0002;
    const OTHER_NPC: u32 = 0x0002_0003;
    let script = parsed_source(
        "ScriptName QF_VmFavor extends Quest\n\
         Actor Property Friend Auto\n\
         Actor Property Common Auto\n\
         Actor Property Other Auto\n\
         GlobalVariable Property Favor Auto\n\
         Function Fragment_1()\n\
           Friend.ModFavorPoints(0)\n\
           Common.ModFavorPoints(50)\n\
           Friend.ModFavorPoints(50)\n\
           Other.ModFavorPointsWithGlobal(Favor)\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.register::<FormIdComponent>();
    let mut index = EsmIndex::default();
    index.npcs.insert(UNIQUE_NPC, NpcRecord { form_id: UNIQUE_NPC, acbs_flags: 0x20, ..Default::default() });
    index.npcs.insert(COMMON_NPC, NpcRecord { form_id: COMMON_NPC, acbs_flags: 0, ..Default::default() });
    index.npcs.insert(OTHER_NPC, NpcRecord { form_id: OTHER_NPC, acbs_flags: 0x20, ..Default::default() });
    crate::actor_base_traits::install_actor_base_traits(&mut world, &index);
    world.insert_resource(crate::Globals([(FAVOR_GLOBAL, 25.0)].into_iter().collect()));
    crate::relationships::install_relationships(&mut world, &index);
    let mut pool = FormIdPool::default();
    let mut spawn = |world: &mut World, local: u32, base: u32| {
        let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(local) });
        let entity = world.spawn();
        world.insert(entity, FormIdComponent(id));
        world.insert(entity, crate::scene::SceneAliasCandidate { base_form_id: base, ..Default::default() });
        entity
    };
    let friend = spawn(&mut world, FRIEND, UNIQUE_NPC);
    let common = spawn(&mut world, FOE, COMMON_NPC);
    let other = spawn(&mut world, OTHER_UNIQUE, OTHER_NPC);
    world.insert_resource(pool);
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        let mut vmad = vmad();
        for (name, form_id) in [("Friend", FRIEND), ("Common", FOE), ("Other", OTHER_UNIQUE), ("Favor", FAVOR_GLOBAL)] {
            vmad.scripts[0].properties.push(ScriptProperty {
                name: name.into(),
                status: 1,
                value: PropertyValue::Object { form_id, alias: -1 },
            });
        }
        fragments.insert_vmad(Q, vmad);
    }
    let player = world.resource::<PapyrusPlayerEntity>().0;
    assert_eq!(crate::relationships::relationship_rank(&world, friend, player), 0);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(crate::relationships::relationship_rank(&world, friend, player), 1, "the unique actor became a friend");
    assert_eq!(crate::relationships::relationship_rank(&world, common, player), 0, "a common actor stays as it was");
    assert_eq!(
        crate::relationships::relationship_rank(&world, other, player),
        1,
        "ModFavorPointsWithGlobal(g) is ModFavorPoints(g.GetValueInt()): the global holds 25"
    );
}

/// `SetBribed` / `SetIntimidated` flag the actor (visible to `IsBribed` in the
/// same run and afterwards), and `ChangeAnimArchetype` records the keyword.
#[test]
fn speech_outcomes_and_anim_archetype_are_recorded() {
    const ARCHETYPE: u32 = 0x0002_2E49;
    let script = parsed_source(
        "ScriptName QF_VmSpeech extends Quest\n\
         Actor Property Guard Auto\n\
         Keyword Property Confident Auto\n\
         Function Fragment_1()\n\
           If !Guard.IsBribed()\n\
             Guard.SetBribed(True)\n\
             Guard.SetIntimidated()\n\
             If Guard.IsBribed() && Guard.IsIntimidated() && Guard.ChangeAnimArchetype(Confident)\n\
               Self.SetStage(20)\n\
             EndIf\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    let property = |name: &str, form_id| ScriptProperty {
        name: name.into(),
        status: 1,
        value: PropertyValue::Object { form_id, alias: -1 },
    };
    world.resource_mut::<crate::QuestStageFragments>().insert_vmad(
        Q,
        ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "QF_VmSpeech".into(),
                status: 0,
                properties: vec![property("Guard", MARKER), property("Confident", ARCHETYPE)],
            }],
        },
    );
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(MARKER) });
    world.insert_resource(pool);
    let guard = world.spawn();
    world.insert(guard, FormIdComponent(id));
    world.insert_resource(crate::SpeechcraftOutcomes::default());
    world.insert_resource(crate::AnimArchetypeOverrides::default());
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
    let outcomes = world.resource::<crate::SpeechcraftOutcomes>();
    assert!(outcomes.is(MARKER, crate::SpeechcraftOutcome::Bribed));
    assert!(outcomes.is(MARKER, crate::SpeechcraftOutcome::Intimidated));
    assert_eq!(world.resource::<crate::AnimArchetypeOverrides>().get(MARKER), Some(ARCHETYPE));
}

/// `AddToMapScanned(b)` is `SetScanned(true)` then `AddToMap(b)`; `IsScanned`
/// reads the flag.
#[test]
fn add_to_map_scanned_marks_scanned_and_adds_to_the_map() {
    let script = parsed_source(
        "ScriptName QF_VmScan extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n\
           If !Marker.IsScanned()\n\
             Marker.AddToMapScanned(True)\n\
             If Marker.IsScanned()\n\
               Self.SetStage(20)\n\
             EndIf\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.resource_mut::<crate::QuestStageFragments>().insert_vmad(
        Q,
        ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "QF_VmScan".into(),
                status: 0,
                properties: vec![ScriptProperty {
                    name: "Marker".into(),
                    status: 1,
                    value: PropertyValue::Object { form_id: MARKER, alias: -1 },
                }],
            }],
        },
    );
    world.insert_resource(crate::ScannedReferences::default());
    world.insert_resource(crate::MapMarkers::default());
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
    assert!(world.resource::<crate::ScannedReferences>().is_scanned(MARKER));
    assert!(crate::map_marker_visible(&world, MARKER) && crate::map_marker_fast_travel(&world, MARKER));
}

/// `ConditionForm.IsTrue(ref, target)` evaluates a loaded condition form with
/// the first reference as subject, and stops the run on a form it cannot model.
#[test]
fn condition_form_is_true_evaluates_modeled_forms() {
    use byroredux_plugin::esm::records::condition::{ComparisonOp, Condition, ConditionValue};
    use byroredux_plugin::esm::records::CndfRecord;
    const FORM: u32 = 0x0001_0F71;
    const GLOBAL: u32 = 0x0001_0F72;
    let script = parsed_source(
        "ScriptName QF_VmCndf extends Quest\n\
         ConditionForm Property Gate Auto\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n\
           If Gate.IsTrue(Marker, Game.GetPlayer())\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    let property = |name: &str, form_id| ScriptProperty {
        name: name.into(),
        status: 1,
        value: PropertyValue::Object { form_id, alias: -1 },
    };
    world.resource_mut::<crate::QuestStageFragments>().insert_vmad(
        Q,
        ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "QF_VmCndf".into(),
                status: 0,
                properties: vec![property("Gate", FORM), property("Marker", MARKER)],
            }],
        },
    );
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Starfield.esm"), local: LocalFormId(MARKER) });
    world.insert_resource(pool);
    let marker = world.spawn();
    world.insert(marker, FormIdComponent(id));
    let mut index = byroredux_plugin::esm::records::EsmIndex::default();
    index.condition_forms.insert(
        FORM,
        CndfRecord {
            form_id: FORM,
            conditions: vec![Condition {
                function_index: 74,
                param_1: GLOBAL,
                comparator: ComparisonOp::Eq,
                comparand: ConditionValue::Literal(1.0),
                ..Default::default()
            }],
            ..Default::default()
        },
    );
    crate::install_condition_forms(&mut world, &index);
    world.insert_resource(crate::Globals::default());
    let run = |world: &World| {
        world.resource_mut::<QuestStageState>().set_stage(Q, 10);
        crate::fragment::quest_fragment_dispatch_system(world);
        world.resource::<QuestStageState>().get_stage(Q)
    };
    assert_eq!(run(&world), 10, "the global is 0");
    world.resource_mut::<crate::Globals>().set(GLOBAL, 1.0);
    assert_eq!(run(&world), 20, "the global is 1");
}

/// `ApplyHavokImpulse` queues the normalized direction times the magnitude on the
/// reference; without the game's impulse settings the run stops instead.
#[test]
fn apply_havok_impulse_queues_a_request() {
    use byroredux_plugin::esm::{reader::GameKind, records::EsmIndex};
    let script = parsed_source(
        "ScriptName QF_VmImpulse extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n\
           Marker.ApplyHavokImpulse(0.0, 0.0, -1.0, 15.0)\n\
           Self.SetStage(20)\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.resource_mut::<crate::QuestStageFragments>().insert_vmad(
        Q,
        ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "QF_VmImpulse".into(),
                status: 0,
                properties: vec![ScriptProperty {
                    name: "Marker".into(),
                    status: 1,
                    value: PropertyValue::Object { form_id: MARKER, alias: -1 },
                }],
            }],
        },
    );
    world.register::<FormIdComponent>();
    world.register::<crate::HavokImpulseRequest>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(MARKER) });
    world.insert_resource(pool);
    let marker = world.spawn();
    world.insert(marker, FormIdComponent(id));
    let run = |world: &World| {
        world.resource_mut::<QuestStageState>().set_stage(Q, 10);
        crate::fragment::quest_fragment_dispatch_system(world);
        world.resource::<QuestStageState>().get_stage(Q)
    };
    assert_eq!(run(&world), 10, "no impulse settings: the run stops");
    crate::install_havok_impulse_rules(&mut world, &EsmIndex { game: GameKind::Skyrim, ..Default::default() });
    assert_eq!(run(&world), 20);
    let pending = world.get::<crate::HavokImpulseRequest>(marker).expect("queued");
    assert_eq!(pending.vector, [0.0, 0.0, -15.0]);
}

/// `PlayAnimation` sends the event to the object's behavior graph: true when the
/// graph took it, with the new state's clip on the object's animation player;
/// `PlayAnimationAndWait` is a latent statement only.
#[test]
fn play_animation_drives_the_objects_behavior_graph() {
    use byroredux_core::animation::AnimationPlayer;
    use byroredux_hkx::{BehaviorGraph, ClipMode, State, StateClip, StateMachine, Transition};
    let script = parsed_source(
        "ScriptName QF_VmGraph extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n\
           If !Marker.PlayAnimation(\"Nope\") && Marker.PlayAnimation(\"Go\")\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n\
         Function Fragment_2()\n\
           Marker.PlayAnimationAndWait(\"Go\", \"Done\")\n\
           Self.SetStage(30)\n\
         EndFunction\n\
         Function Fragment_3()\n\
           If Marker.PlayAnimationAndWait(\"Go\", \"Done\")\n\
             Self.SetStage(30)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_2", None).err(), None, "latent as a statement");
    assert!(check_function_reason(Arc::clone(&script), "Fragment_3", None).is_err(), "latent for its value is refused");
    let mut world = world_with_fragment(script);
    world.resource_mut::<crate::QuestStageFragments>().insert_vmad(
        Q,
        ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "QF_VmGraph".into(),
                status: 0,
                properties: vec![ScriptProperty {
                    name: "Marker".into(),
                    status: 1,
                    value: PropertyValue::Object { form_id: MARKER, alias: -1 },
                }],
            }],
        },
    );
    world.register::<FormIdComponent>();
    world.register::<AnimationPlayer>();
    world.register::<crate::ObjectAnimationGraph>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(MARKER) });
    world.insert_resource(pool);
    let marker = world.spawn();
    world.insert(marker, FormIdComponent(id));
    let clip = StateClip {
        animation: "a/go.hkx".into(),
        mode: ClipMode::SinglePlay,
        playback_speed: 1.0,
        start_time: 0.0,
        user_fraction: 0.0,
        triggers: Vec::new(),
            playback_speed_variable: None,
    };
    let transition = Transition { event: 0, to_state: 1, priority: 0, flags: 0, ..Default::default() };
    let graph = BehaviorGraph {
        variables: Vec::new(),
        events: vec!["Go".into(), "Done".into()],
        machine: Some(StateMachine {
            start_state: 0,
            start_state_variable: None,
            states: vec![
                State { id: 0, name: "Idle".into(), transitions: vec![transition], clip: None, content: Default::default() },
                State { id: 1, name: "Going".into(), transitions: Vec::new(), clip: Some(clip), content: Default::default() },
            ],
            wildcard: Vec::new(),
        }),
    };
    let mut clips = std::collections::HashMap::new();
    clips.insert(("a/go.hkx".to_owned(), false), crate::GraphClip { handle: 4, duration: 1.0 });
    world.insert(
        marker,
        crate::ObjectAnimationGraph::new(Arc::new(crate::LoadedGraph { graph, clips, sequences: Default::default(), lazy: None }), marker).unwrap(),
    );
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20, "the graph took Go and refused Nope");
    let player = world.get::<AnimationPlayer>(marker).expect("the Going clip is playing");
    assert_eq!((player.clip_handle, player.root_entity), (4, Some(marker)));
}

/// `WaitForAnimationEvent` suspends the script until the object's graph raises
/// the event (a clip trigger 0.5 s in), and returns at once for an object with
/// no graph (the CK's "graph unloaded"). It is latent: a statement only.
#[test]
fn wait_for_animation_event_waits_for_the_graphs_event() {
    use byroredux_core::animation::AnimationPlayer;
    use byroredux_hkx::{BehaviorGraph, ClipMode, ClipTrigger, State, StateClip, StateMachine, Transition};
    const SOURCE: &str = "ScriptName QF_VmWaitEvent extends Quest\n\
        ObjectReference Property Marker Auto\n\
        Function Fragment_1()\n\
          Marker.PlayAnimation(\"Go\")\n\
          Marker.WaitForAnimationEvent(\"Done\")\n\
          Self.SetStage(20)\n\
        EndFunction\n\
        Function Fragment_2()\n\
          If Marker.WaitForAnimationEvent(\"Done\")\n\
            Self.SetStage(20)\n\
          EndIf\n\
        EndFunction\n";
    let script = parsed_source(SOURCE);
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    assert!(check_function_reason(Arc::clone(&script), "Fragment_2", None).is_err(), "latent for its value is refused");
    let build = |with_graph: bool| {
        let mut world = world_with_fragment(Arc::clone(&script));
        world.resource_mut::<crate::QuestStageFragments>().insert_vmad(
            Q,
            ScriptInstanceData {
                version: 5,
                object_format: 2,
                scripts: vec![ScriptInstance {
                    name: "QF_VmWaitEvent".into(),
                    status: 0,
                    properties: vec![ScriptProperty {
                        name: "Marker".into(),
                        status: 1,
                        value: PropertyValue::Object { form_id: MARKER, alias: -1 },
                    }],
                }],
            },
        );
        world.register::<FormIdComponent>();
        world.register::<AnimationPlayer>();
        world.register::<crate::ObjectAnimationGraph>();
        let mut pool = FormIdPool::default();
        let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(MARKER) });
        world.insert_resource(pool);
        let marker = world.spawn();
        world.insert(marker, FormIdComponent(id));
        if with_graph {
            let clip = StateClip {
                animation: "a/go.hkx".into(),
                mode: ClipMode::SinglePlay,
                playback_speed: 1.0,
                start_time: 0.0,
                user_fraction: 0.0,
                triggers: vec![ClipTrigger { local_time: 0.5, event: 1, relative_to_end: false, annotation: false }],
            playback_speed_variable: None,
            };
            let graph = BehaviorGraph {
                variables: Vec::new(),
                events: vec!["Go".into(), "Done".into()],
                machine: Some(StateMachine {
                    start_state: 0,
                    start_state_variable: None,
                    states: vec![
                        State { id: 0, name: "Idle".into(), transitions: vec![Transition { event: 0, to_state: 1, priority: 0, flags: 0, ..Default::default() }], clip: None, content: Default::default() },
                        State { id: 1, name: "Going".into(), transitions: Vec::new(), clip: Some(clip), content: Default::default() },
                    ],
                    wildcard: Vec::new(),
                }),
            };
            let mut clips = std::collections::HashMap::new();
            clips.insert(("a/go.hkx".to_owned(), false), crate::GraphClip { handle: 4, duration: 1.0 });
            world.insert(
                marker,
                crate::ObjectAnimationGraph::new(Arc::new(crate::LoadedGraph { graph, clips, sequences: Default::default(), lazy: None }), marker).unwrap(),
            );
        }
        world.resource_mut::<QuestStageState>().set_stage(Q, 10);
        crate::fragment::quest_fragment_dispatch_system(&world);
        world
    };
    let world = build(true);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 10, "waiting for Done");
    crate::fragment::fragment_continuation_system(&world, 0.4);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 10, "0.1 s short");
    crate::fragment::fragment_continuation_system(&world, 0.2);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20, "Done raised at 0.5 s");
    let world = build(false);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20, "no graph: no wait");
}

/// `Utility.GetCurrentRealTime` is the engine's wall-clock seconds since start.
#[test]
fn get_current_real_time_reads_the_engine_clock() {
    let script = parsed_source(
        "ScriptName QF_VmRealTime extends Quest\n\
         Function Fragment_1()\n\
           Float started = Utility.GetCurrentRealTime()\n\
           If started > 11.5 && started < 12.5\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.insert_resource(byroredux_core::ecs::TotalTime(12.0));
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// GUI-REG-011 — `Utility.IsInMenuMode` is true exactly while the menu registry holds a menu that
/// pauses the game: the inventory does, the dialogue menu and the HUD do not.
#[test]
fn is_in_menu_mode_reads_the_menu_registry() {
    use byroredux_core::ecs::resources::menu_registry::{MenuGame, MenuRegistry, MenuSurface};
    let script = parsed_source(
        "ScriptName QF_VmMenuMode extends Quest\n\
         Function Fragment_1()\n\
           If Utility.IsInMenuMode()\n\
             Self.SetStage(20)\n\
           Else\n\
             Self.SetStage(30)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let page = |name: &str| (name.to_owned(), MenuSurface::Native { page: "test".into() });
    let answer = |open: &[&str]| {
        let mut world = world_with_fragment(Arc::clone(&script));
        let mut registry = MenuRegistry::new(MenuGame::Skyrim);
        registry.publish(open.iter().map(|name| page(name)));
        world.insert_resource(registry);
        world.resource_mut::<QuestStageState>().set_stage(Q, 10);
        crate::fragment::quest_fragment_dispatch_system(&world);
        world.resource::<QuestStageState>().get_stage(Q)
    };
    assert_eq!(answer(&["HUD Menu", "InventoryMenu"]), 20, "the inventory pauses the game");
    assert_eq!(answer(&["HUD Menu", "Dialogue Menu"]), 30, "the dialogue menu does not");
    assert_eq!(answer(&["HUD Menu"]), 30, "nor the HUD alone");
}

/// `IsInDialogueWithPlayer` is true for the NPC whose response surface is open.
#[test]
fn is_in_dialogue_with_player_follows_the_open_surface() {
    let script = parsed_source(
        "ScriptName QF_VmTalk extends Quest\n\
         Actor Property Marker Auto\n\
         Function Fragment_1()\n\
           If Marker.IsInDialogueWithPlayer()\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(MARKER) });
    world.insert_resource(pool);
    let marker = world.spawn();
    world.insert(marker, FormIdComponent(id));
    let run = |world: &World| {
        world.resource_mut::<QuestStageState>().set_stage(Q, 10);
        crate::fragment::quest_fragment_dispatch_system(world);
        world.resource::<QuestStageState>().get_stage(Q)
    };
    assert_eq!(run(&world), 10, "no conversation");
    let stranger = world.spawn();
    world.insert_resource(crate::DialogueSurfaceState { npc: Some(stranger), ..Default::default() });
    assert_eq!(run(&world), 10, "someone else is talking");
    world.insert_resource(crate::DialogueSurfaceState { npc: Some(marker), ..Default::default() });
    assert_eq!(run(&world), 20, "the marker is talking to the player");
}

/// A perk entry run runs the entry's fragment with `akTargetRef` and `akActor`
/// bound; another entry's fragment does not run.
#[test]
fn perk_entry_run_executes_the_entrys_fragment() {
    const PERK: u32 = 0x0001_0F60;
    let script = parsed_source(
        "ScriptName PRKF_VmTest extends Perk\n\
         Function Fragment_Entry_00(ObjectReference akTargetRef, Actor akActor)\n\
           akTargetRef.Disable()\n\
         EndFunction\n\
         Function Fragment_Entry_01(ObjectReference akTargetRef, Actor akActor)\n\
           akTargetRef.Enable()\n\
         EndFunction\n",
    );
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    let installed = std::cell::RefCell::new(Vec::new());
    let library: HashMap<String, Arc<Script>> = HashMap::new();
    let library = Installed(&library);
    {
        let mut fragments = world.resource_mut::<crate::PerkFragments>();
        let n = crate::populate_fragments(
            &mut fragments,
            PERK,
            None,
            script,
            &[(crate::PerkEntryId(0), "Fragment_Entry_00"), (crate::PerkEntryId(1), "Fragment_Entry_01")],
            &library,
            &mut |s| installed.borrow_mut().push(s),
        );
        assert_eq!(n, 2);
    }
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        for script in installed.into_inner() {
            fragments.insert_vm_script(script);
        }
    }
    let run = |entry| crate::PerkEntryRun { perk: PERK, entry, owner: 0x14, target: Some(MARKER) };
    crate::report_perk_entry(&world, run(0));
    crate::perk_fragment_system(&world, 0.0);
    assert!(!crate::fragment::reference_enabled(&world, MARKER), "entry 0 disabled the target");
    crate::report_perk_entry(&world, run(1));
    crate::perk_fragment_system(&world, 0.0);
    assert!(crate::fragment::reference_enabled(&world, MARKER), "entry 1 enabled it again");
    crate::report_perk_entry(&world, run(2));
    crate::perk_fragment_system(&world, 0.0);
    assert!(crate::fragment::reference_enabled(&world, MARKER), "no fragment for entry 2");
}

/// `IsFurnitureInUse` is true while an actor sits on the furniture, or (unless
/// reservations are ignored) while a seat is claimed; `IsStarting` /
/// `IsStopping` are false for a quest that starts and stops in one step.
#[test]
fn furniture_in_use_and_quest_transition_states() {
    use byroredux_core::ecs::components::{SeatReservations, Seated};
    let script = parsed_source(
        "ScriptName QF_VmFurniture extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n\
           If !Self.IsStarting() && !Self.IsStopping()\n\
             If Marker.IsFurnitureInUse(True)\n\
               Self.SetStage(30)\n\
             ElseIf Marker.IsFurnitureInUse()\n\
               Self.SetStage(20)\n\
             EndIf\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.register::<FormIdComponent>();
    world.register::<Seated>();
    world.insert_resource(SeatReservations::default());
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(MARKER) });
    world.insert_resource(pool);
    let furniture = world.spawn();
    world.insert(furniture, FormIdComponent(id));
    let actor = world.spawn();
    let run = |world: &World| {
        world.resource_mut::<QuestStageState>().set_stage(Q, 5);
        world.resource_mut::<QuestStageState>().set_stage(Q, 10);
        crate::fragment::quest_fragment_dispatch_system(world);
        world.resource::<QuestStageState>().get_stage(Q)
    };
    assert_eq!(run(&world), 10, "empty furniture");
    world.resource_mut::<SeatReservations>().0.insert((furniture, 0), actor);
    assert_eq!(run(&world), 20, "a claimed seat counts unless reservations are ignored");
    world.insert(
        actor,
        Seated { furniture, animation_restore: Default::default() },
    );
    assert_eq!(run(&world), 30, "an actor sits on it");
}

/// `StopCombatAlarm` stands down every actor fighting the receiver, and no one else.
#[test]
fn stop_combat_alarm_stands_down_those_fighting_the_receiver() {
    let script = parsed_source(
        "ScriptName QF_VmAlarm extends Quest\n\
         Function Fragment_1()\n\
           Game.GetPlayer().StopCombatAlarm()\n\
           Self.SetStage(20)\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let world = world_with_fragment(script);
    let player = world.resource::<PapyrusPlayerEntity>().0;
    let mut world = world;
    let (hunter, bystander, other) = (world.spawn(), world.spawn(), world.spawn());
    let fight = |target| crate::AiCombatState { target, attack_cooldown_remaining: 0.0 };
    world.insert(hunter, fight(player));
    world.insert(bystander, fight(other));
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
    assert!(world.get::<crate::AiCombatState>(hunter).is_none(), "the player's hunter stood down");
    assert!(world.get::<crate::AiCombatState>(bystander).is_some(), "another fight is untouched");
}

/// `GetCrimeFaction` reads the base's `CRIF`, `SetCrimeFaction` overrides it (and
/// adds the actor to the faction), and `None` clears it.
#[test]
fn crime_faction_is_authored_then_scripted() {
    use byroredux_core::ecs::components::FactionRanks;
    use byroredux_plugin::esm::records::NpcRecord;
    const BASE: u32 = 0x0001_3BA0;
    const AUTHORED: u32 = 0x0002_8170;
    let script = parsed_source(
        "ScriptName QF_VmCrime extends Quest\n\
         Actor Property Marker Auto\n\
         Faction Property Alias_Guard Auto\n\
         Function Fragment_1()\n\
           If Marker.GetCrimeFaction()\n\
             Marker.SetCrimeFaction(None)\n\
             If !Marker.GetCrimeFaction()\n\
               Self.SetStage(20)\n\
             EndIf\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.register::<FormIdComponent>();
    world.register::<FactionRanks>();
    let mut index = byroredux_plugin::esm::records::EsmIndex::default();
    index.npcs.insert(BASE, NpcRecord { form_id: BASE, crime_faction_form_id: AUTHORED, ..Default::default() });
    crate::install_crime_factions(&mut world, &index);
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(MARKER) });
    let base_id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(BASE) });
    world.insert_resource(pool);
    let marker = world.spawn();
    world.insert(marker, FormIdComponent(id));
    world.register::<crate::SceneAliasCandidate>();
    world.insert(
        marker,
        crate::SceneAliasCandidate { reference_form_id: MARKER, base_form_id: BASE, ..Default::default() },
    );
    let _ = base_id;
    assert_eq!(crate::crime_faction(&world, marker, MARKER), Some(AUTHORED));
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20, "read, cleared, read again");
    assert_eq!(crate::crime_faction(&world, marker, MARKER), None, "the override cleared it");
}

/// `DropObject` takes up to the count from the owner's inventory and creates a
/// reference per item dropped; an item the inventory lacks drops nothing.
#[test]
fn drop_object_moves_inventory_items_into_the_world() {
    use byroredux_core::ecs::components::{Inventory, ItemStack};
    let script = parsed_source(
        "ScriptName QF_VmDrop extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Form Property Alias_Item Auto\n\
         Function Fragment_1()\n\
           ObjectReference dropped = Marker.DropObject(Marker, 3)\n\
           If dropped\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.register::<FormIdComponent>();
    world.register::<Inventory>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(MARKER) });
    world.insert_resource(pool);
    let owner = world.spawn();
    world.insert(owner, FormIdComponent(id));
    // The owner holds two of MARKER (the item is the marker form itself here).
    let mut inventory = Inventory::new();
    inventory.push(ItemStack::new(MARKER, 2));
    world.insert(owner, inventory);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
    let left: u32 = world.get::<Inventory>(owner).unwrap().items.iter().map(|stack| stack.count).sum();
    assert_eq!(left, 0, "two were held, so two left (3 asked)");
    assert_eq!(world.resource::<crate::CreatedReferences>().iter().count(), 2, "one reference per dropped item");
}

/// Starfield guards, as the decompiler lifts them (`Guard.lock(Name)` and
/// `Guard.unlock(Name)`), pass: the arguments name guards, not values.
#[test]
fn guard_operations_do_not_evaluate_their_guard_names() {
    let script = parsed_source(
        "ScriptName QF_VmGuard extends Quest\n\
         Function Fragment_1()\n\
           Guard.lock(SomeDataGuard, OtherGuard)\n\
           Self.SetStage(20)\n\
           Guard.unlock(SomeDataGuard, OtherGuard)\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let world = world_with_fragment(script);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// `IsInLocation` is true for the location the reference is in and for any
/// location above it (CK *IsInLocation*: the location or a child of it), and
/// false for another branch.
#[test]
fn is_in_location_includes_the_ancestors() {
    use crate::location::tests::{whiterun_index, DRAGONSREACH_CELL, TAMRIEL, WHITERUN_HOLD};
    use byroredux_core::ecs::components::{CellFormId, CellRoot};
    let script = parsed_source(
        "ScriptName QF_VmInLoc extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Location Property Hold Auto\n\
         Location Property Elsewhere Auto\n\
         Function Fragment_1()\n\
           If Marker.IsInLocation(Hold) && !Marker.IsInLocation(Elsewhere)\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    {
        let mut vmad = vmad();
        let object = |name: &str, form_id: u32| ScriptProperty {
            name: name.into(),
            status: 1,
            value: PropertyValue::Object { form_id, alias: -1 },
        };
        vmad.scripts[0].properties.push(object("Hold", WHITERUN_HOLD));
        vmad.scripts[0].properties.push(object("Elsewhere", TAMRIEL + 1));
        world.resource_mut::<crate::QuestStageFragments>().insert_vmad(Q, vmad);
    }
    crate::install_location_table(&mut world, &whiterun_index());
    world.register::<FormIdComponent>();
    world.register::<CellFormId>();
    world.register::<CellRoot>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(MARKER) });
    world.insert_resource(pool);
    let cell = world.spawn();
    world.insert(cell, CellFormId(DRAGONSREACH_CELL));
    let marker = world.spawn();
    world.insert(marker, FormIdComponent(id));
    world.insert(marker, CellRoot(cell));
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20, "Dragonsreach is within the hold");
}

/// `IgnoreFriendlyHits` and `SetCanDoCommand` record their flags (default True),
/// and `Game.StopDialogueCamera` has no camera to stop.
#[test]
fn reference_flags_and_the_dialogue_camera() {
    use crate::ReferenceFlag;
    let script = parsed_source(
        "ScriptName QF_VmFlags extends Quest\n\
         Actor Property Marker Auto\n\
         Function Fragment_1()\n\
           Marker.IgnoreFriendlyHits()\n\
           Marker.SetCanDoCommand(False)\n\
           Game.StopDialogueCamera(False, False)\n\
           Self.SetStage(20)\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let world = world_with_fragment(script);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
    let flags = world.resource::<crate::ReferenceFlags>();
    assert!(flags.is_set(MARKER, ReferenceFlag::IgnoreFriendlyHits));
    assert!(!flags.is_set(MARKER, ReferenceFlag::CanDoCommand));
}

/// `GetActorBase().GetSex()` reads the base's female flag and `GetVoiceType()`
/// its `VTCK` (CK *GetSex - ActorBase*: 0 Male, 1 Female).
#[test]
fn sex_and_voice_type_come_from_the_actor_base() {
    use byroredux_plugin::esm::records::NpcRecord;
    const BASE: u32 = 0x0001_3BA1;
    const VOICE: u32 = 0x0002_9F00;
    let script = parsed_source(
        "ScriptName QF_VmSex extends Quest\n\
         Actor Property Marker Auto\n\
         Function Fragment_1()\n\
           If Marker.GetActorBase().GetSex() == 1 && Marker.GetVoiceType()\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    let mut index = byroredux_plugin::esm::records::EsmIndex::default();
    index.npcs.insert(
        BASE,
        NpcRecord { form_id: BASE, acbs_flags: 1, voice_form_id: VOICE, ..Default::default() },
    );
    crate::install_actor_base_traits(&mut world, &index);
    world.register::<FormIdComponent>();
    world.register::<crate::SceneAliasCandidate>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(MARKER) });
    world.insert_resource(pool);
    let marker = world.spawn();
    world.insert(marker, FormIdComponent(id));
    world.insert(
        marker,
        crate::SceneAliasCandidate { reference_form_id: MARKER, base_form_id: BASE, ..Default::default() },
    );
    assert_eq!(crate::voice_type(&world, marker), Some(VOICE));
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// `SetFactionOwner` replaces the owner (the run sees it at once), `SetActorOwner(None)`
/// clears it, and crime gold follows `SetCrimeGold` / `ModCrimeGold` with the
/// CK's two kinds (`GetCrimeGold` is both).
#[test]
fn owners_and_crime_gold_follow_their_setters() {
    const FACTION: u32 = 0x0001_0F78;
    let script = parsed_source(
        "ScriptName QF_VmOwn2 extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Faction Property Owners Auto\n\
         Function Fragment_1()\n\
           Marker.SetFactionOwner(Owners)\n\
           If Marker.GetFactionOwner() == Owners\n\
             Marker.SetActorOwner(None)\n\
             Owners.SetCrimeGold(40)\n\
             Owners.ModCrimeGold(25, True)\n\
             Owners.ModCrimeGold(5)\n\
             If !Marker.GetFactionOwner() && Owners.GetCrimeGold() == 70 && Owners.GetCrimeGoldViolent() == 25 && Owners.GetCrimeGoldNonViolent() == 45\n\
               Self.SetStage(20)\n\
             EndIf\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    let object = |name: &str, form_id: u32| ScriptProperty {
        name: name.into(),
        status: 1,
        value: PropertyValue::Object { form_id, alias: -1 },
    };
    world.resource_mut::<crate::QuestStageFragments>().insert_vmad(
        Q,
        ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "QF_VmOwn2".into(),
                status: 0,
                properties: vec![object("Marker", MARKER), object("Owners", FACTION)],
            }],
        },
    );
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(MARKER) });
    world.insert_resource(pool);
    let marker = world.spawn();
    world.insert(marker, FormIdComponent(id));
    let mut index = byroredux_plugin::esm::records::EsmIndex::default();
    index.factions.insert(
        FACTION,
        byroredux_plugin::esm::records::FactionRecord {
            form_id: FACTION,
            editor_id: String::new(),
            full_name: String::new(),
            flags: 0,
            relations: Vec::new(),
            ranks: Vec::new(),
            reputation: None,
            vendor: None,
            crime: None,
        },
    );
    crate::install_owner_kinds(&mut world, &index);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
    assert_eq!(world.resource::<crate::OwnerOverrides>().get(MARKER), Some(0), "the clear is kept");
    assert_eq!(world.resource::<crate::FactionCrimeGold>().of(FACTION), (45, 25));
}

/// `Faction.SendPlayerToJail` runs: `ARRT` is raised (with the player as the arrested actor) and a
/// fake jail (`abRealJail = False`) moves nobody.
#[test]
fn send_player_to_jail_raises_the_arrest_event() {
    const FACTION: u32 = 0x0001_0F78;
    let script = parsed_source(
        "ScriptName QF_VmJail extends Quest\n\
         Faction Property Police Auto\n\
         Function Fragment_1()\n Police.SendPlayerToJail(True, False)\n EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.resource_mut::<crate::QuestStageFragments>().insert_vmad(
        Q,
        ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "QF_VmJail".into(),
                status: 0,
                properties: vec![ScriptProperty { name: "Police".into(), status: 1, value: PropertyValue::Object { form_id: FACTION, alias: -1 } }],
            }],
        },
    );
    let mut index = byroredux_plugin::esm::records::EsmIndex::default();
    index.factions.insert(
        FACTION,
        byroredux_plugin::esm::records::FactionRecord {
            form_id: FACTION,
            editor_id: String::new(),
            full_name: String::new(),
            flags: 0,
            relations: Vec::new(),
            ranks: Vec::new(),
            reputation: None,
            vendor: None,
            crime: None,
        },
    );
    crate::install_owner_kinds(&mut world, &index);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    let arrests = world
        .query::<crate::story_manager::StoryEvent>()
        .map(|events| events.iter().filter(|(_, e)| e.mnemonic == *b"ARRT").count())
        .unwrap_or(0);
    assert_eq!(arrests, 1);
    assert!(world.resource::<crate::PlayerMoveRequest>().0.is_none(), "a fake jail moves nobody");
}

/// A function of a script on an alias, reached through a cast of the alias
/// property, runs with an alias as `Self`: its `GetActorRef()` checks.
#[test]
fn a_cast_alias_script_function_checks_with_an_alias_self() {
    let helper = parsed_source(
        "ScriptName GuardHelper extends ReferenceAlias\n\
         Function Poke()\n\
           Actor who = Self.GetActorRef()\n\
           If who\n\
             who.Disable()\n\
           EndIf\n\
         EndFunction\n",
    );
    let script = parsed_source(
        "ScriptName QF_VmAliasCast extends Quest\n\
         ReferenceAlias Property Alias_Guard Auto\n\
         Function Fragment_1()\n\
           (Alias_Guard as GuardHelper).Poke()\n\
         EndFunction\n",
    );
    let scripts: HashMap<String, Arc<Script>> = [("guardhelper".to_owned(), helper)].into();
    let library = Installed(&scripts);
    assert_eq!(check_function_reason(script, "Fragment_1", Some(&library)).err(), None);
}

/// `GetLinkedRefChain` lists the chain after the reference and stops at a
/// loop or the cap; `GetLinkedRefChildren` lists the loaded references linked to
/// it.
#[test]
fn linked_ref_chain_and_children() {
    const SECOND: u32 = 0x0002_0001;
    const THIRD: u32 = 0x0002_0002;
    let script = parsed_source(
        "ScriptName QF_VmChain extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n\
           ObjectReference[] chain = Marker.GetLinkedRefChain(None, 100)\n\
           ObjectReference[] capped = Marker.GetLinkedRefChain(None, 1)\n\
           If chain.Length == 2 && capped.Length == 1\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let world = world_with_fragment(script);
    {
        let mut links = world.resource_mut::<crate::LinkedRefOverrides>();
        links.set(MARKER, 0, SECOND);
        links.set(SECOND, 0, THIRD);
        links.set(THIRD, 0, MARKER); // a loop back to the start
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// `CountLinkedRefChain` follows the CK page's script: a chain A to B to C
/// counts 3 (the missing last link counts too), a chain that links back to the
/// start does not count that last link, and the cap stops it.
#[test]
fn count_linked_ref_chain_follows_the_pages_script() {
    const SECOND: u32 = 0x0002_0001;
    const THIRD: u32 = 0x0002_0002;
    let script = parsed_source(
        "ScriptName QF_VmCount extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n\
           If Marker.CountLinkedRefChain(None, 100) == 3 && Marker.CountLinkedRefChain(None, 0) == 1\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let world = world_with_fragment(script);
    {
        let mut links = world.resource_mut::<crate::LinkedRefOverrides>();
        links.set(MARKER, 0, SECOND);
        links.set(SECOND, 0, THIRD);
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
    // Linking the end back to the start leaves the two real links.
    world.resource_mut::<crate::LinkedRefOverrides>().set(THIRD, 0, MARKER);
    let looped = parsed_source(
        "ScriptName QF_VmCountLoop extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n\
           If Marker.CountLinkedRefChain() == 2\n\
             Self.SetStage(30)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&looped), "Fragment_1", None).err(), None);
    {
        let name = looped.name.node.0.clone();
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        fragments.insert_vm_script(looped);
        fragments.insert(
            Q,
            10,
            vec![Effect::Interpret { script: name, function: "Fragment_1".into(), args: Vec::new(), resume: None }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 5);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 30);
}

/// `PlaceAtMe` creates the references through the saved `CreatedReferences`
/// (FormIDs in the scripted half of the created slot), returns the last one,
/// disables only that one when asked, and flags `DeleteWhenAble`.
#[test]
fn place_at_me_creates_scripted_references() {
    let script = parsed_source(
        "ScriptName QF_VmPlace extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n\
           ObjectReference made = Marker.PlaceAtMe(Marker, 2, False, True, True)\n\
           If made\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let world = world_with_fragment(script);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
    let created = world.resource::<crate::CreatedReferences>();
    let mut made: Vec<_> = created.iter().map(|r| (r.form_id, r.base, r.at_reference, r.deletion)).collect();
    made.sort_unstable_by_key(|r| r.0);
    assert_eq!(
        made,
        vec![
            (0xFF80_0000, MARKER, MARKER, crate::CreatedDeletion::WhenAble),
            (0xFF80_0001, MARKER, MARKER, crate::CreatedDeletion::WhenAble),
        ]
    );
    drop(created);
    let enable = world.resource::<crate::fragment::ReferenceEnableState>();
    assert_eq!(enable.override_for(0xFF80_0001), Some(false), "only the last is disabled");
    assert_eq!(enable.override_for(0xFF80_0000), None);
}

/// `UpdateCurrentInstanceGlobal` as a statement refreshes a cached global
/// copy this engine does not keep: it runs and does nothing; its Bool result
/// used as a value stays declined.
#[test]
fn update_current_instance_global_statement_is_a_no_op() {
    let script = parsed_source(
        "ScriptName QF_VmInstance extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n\
           Int count = 0\n\
           While count < 2\n\
             count += 1\n\
           EndWhile\n\
           Self.UpdateCurrentInstanceGlobal(Marker)\n\
           Self.SetStage(20)\n\
         EndFunction\n\
         Function Fragment_2()\n\
           If Self.UpdateCurrentInstanceGlobal(Marker)\n\
             Self.SetStage(30)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    assert!(check_function_reason(Arc::clone(&script), "Fragment_2", None).is_err());
    let world = world_with_fragment(script);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// `Game.GetForm` answers a loaded plugin's id and None for an unloaded
/// one; `Delete()` flags a created reference like `DeleteWhenAble`.
#[test]
fn get_form_validates_the_plugin_and_delete_flags_created_references() {
    let script = parsed_source(
        "ScriptName QF_VmGetForm extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n\
           If Game.GetForm(0x00010F02) as Bool && !(Game.GetForm(0x7A010F02) as Bool)\n\
             Marker.Enable()\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.insert_resource(crate::load_order::LoadOrderIdentity::new(vec![(
        byroredux_plugin::esm::reader::GlobalSlot::Regular(0),
        PluginId::from_filename("Skyrim.esm"),
    )]));
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert!(crate::fragment::reference_enabled(&world, MARKER));
    let delete = parsed_source(
        "ScriptName QF_VmDelete extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n Marker.Delete()\n EndFunction\n",
    );
    assert_eq!(check_function_reason(delete, "Fragment_1", None).err(), None);
}

/// `SetPlayerTeammate` / `SetDoingFavor` set the saved follower flags,
/// `IsPlayerTeammate` / `IsDoingFavor` read them.
#[test]
fn follower_flags_set_and_read() {
    const BUDDY: u32 = 0x0001_0F40;
    let script = parsed_source(
        "ScriptName QF_VmFollower extends Quest\n\
         Actor Property Buddy Auto\n\
         Function Fragment_1()\n\
           If !Buddy.IsPlayerTeammate()\n\
             Buddy.SetPlayerTeammate(True, False)\n\
             Buddy.SetDoingFavor()\n\
           EndIf\n\
           If Buddy.IsPlayerTeammate() && Buddy.IsDoingFavor()\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(BUDDY),
    });
    world.insert_resource(pool);
    let buddy = world.spawn();
    world.insert(buddy, FormIdComponent(id));
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        let mut vmad = vmad();
        vmad.scripts[0].properties.push(ScriptProperty {
            name: "Buddy".into(),
            status: 1,
            value: PropertyValue::Object {
                form_id: BUDDY,
                alias: -1,
            },
        });
        fragments.insert_vmad(Q, vmad);
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
    let flags = world.resource::<crate::FollowerState>().flags(BUDDY);
    assert!(flags.teammate && !flags.can_do_favor && flags.doing_favor);
}

// ── Session 99 — Papyrus VM per-frame cost census (`#[ignore]`) ─────────────
//
// `cargo test -p byroredux-scripting --release --lib -- --ignored --nocapture vm_cost`
//
// Everything the VM adds to the frame runs in exclusive `Update` systems
// (`reference_script_event_system`, `quest_fragment_dispatch_system`,
// `fragment_continuation_system`, `script_update_system`), so these are
// serial main-thread costs. Four questions, each a table row:
//   A. idle: N attached reference scripts, no events this frame
//   B. burst: one `OnCellLoad` delivered to all N in a single frame
//   C. polling: N scripts on `RegisterForUpdate(interval)`, 60 fps frames
//   D. runaway: one handler that loops until the 100 000-statement cap

fn vm_cost_world(n: usize, source: &str, name: &str) -> (World, Vec<byroredux_core::ecs::EntityId>) {
    let script = parsed_source(source);
    let (events, scripts) =
        crate::check_reference_script(Arc::clone(&script), &crate::DELIVERED_EVENTS, None).expect("the VM runs it");
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let mut refs = Vec::with_capacity(n);
    let ids: Vec<_> = (0..n)
        .map(|i| {
            pool.intern(FormIdPair {
                plugin: PluginId::from_filename("Skyrim.esm"),
                local: LocalFormId(0x0002_0000 + i as u32),
            })
        })
        .collect();
    world.insert_resource(pool);
    let instance = ScriptInstance { name: name.into(), status: 0, properties: Vec::new() };
    for id in ids {
        let r = world.spawn();
        world.insert(r, FormIdComponent(id));
        crate::attach_reference_script(&mut world, r, &instance, events.clone(), scripts.clone());
        refs.push(r);
    }
    (world, refs)
}

fn vm_cost_us<F: FnMut()>(iters: u32, mut f: F) -> f64 {
    for _ in 0..(iters / 10).max(1) {
        f();
    }
    let t = std::time::Instant::now();
    for _ in 0..iters {
        f();
    }
    t.elapsed().as_secs_f64() * 1e6 / f64::from(iters)
}

/// Put `world` in the budgeted state production has (the default allowance and
/// a continuation queue), or strip the budget to measure what ran before it.
fn census_mode(world: &mut World, budgeted: bool) {
    if budgeted {
        world.insert_resource(crate::VmFrameBudget::default());
        world.init_resource::<crate::FragmentExecutionQueue>();
    } else {
        world.remove_resource::<crate::VmFrameBudget>();
    }
}

fn census_frame(world: &World) -> f64 {
    let t = std::time::Instant::now();
    crate::reference_script_event_system(world, 0.0);
    crate::script_update_system(world, 1.0 / 60.0, Some(1.0));
    crate::fragment_continuation_system(world, 1.0 / 60.0);
    let ms = t.elapsed().as_secs_f64() * 1e3;
    crate::event_cleanup_system(world, 0.0);
    ms
}

#[test]
#[ignore = "timing census; run with --release --nocapture"]
fn vm_cost_idle_burst_polling_runaway() {
    const SRC: &str = "ScriptName VmCostScript extends ObjectReference\n\
         Int Property Count Auto\n\
         Event OnInit()\n Count = 10\n EndEvent\n\
         Event OnLoad()\n Count += 1\n EndEvent\n\
         Event OnActivate(ObjectReference akActionRef)\n Count += 1\n EndEvent\n";
    eprintln!("\nA. idle frame (no events), µs per call of each VM system");
    eprintln!("{:>8} {:>14} {:>14} {:>14} {:>14}", "refs", "ref_events", "quest_frag", "continuation", "script_update");
    for n in [0usize, 100, 1000, 5000, 20000] {
        let (world, _refs) = vm_cost_world(n, SRC, "VmCostScript");
        let iters = if n >= 5000 { 200 } else { 2000 };
        let a = vm_cost_us(iters, || crate::reference_script_event_system(&world, 0.016));
        let b = vm_cost_us(iters, || crate::fragment::quest_fragment_dispatch_system(&world));
        let c = vm_cost_us(iters, || crate::fragment_continuation_system(&world, 0.016));
        let d = vm_cost_us(iters, || crate::script_update_system(&world, 0.016, Some(1.0)));
        eprintln!("{n:>8} {a:>14.2} {b:>14.2} {c:>14.2} {d:>14.2}");
        // #5857 — the census's one hard assertion: an idle frame at 20k
        // registered references must stay inside a generous wall. This is
        // not a micro-benchmark gate (CI hardware varies wildly); it exists
        // so a 100x regression (a per-reference allocation, an accidental
        // O(n²)) fails instead of merely printing a bigger table. Measured
        // in the low hundreds of µs per system on the dev machine.
        if n == 20_000 {
            for (system, us) in [("ref_events", a), ("quest_frag", b), ("continuation", c), ("script_update", d)] {
                assert!(
                    us < 5_000.0,
                    "{system} took {us:.0} µs/call at 20 000 idle references — \
                     an order-of-magnitude VM regression"
                );
            }
        }
    }

    eprintln!("\nB. burst: OnCellLoad to every attached reference in one frame");
    eprintln!("{:>8} {:>10} {:>14} {:>14} {:>8} {:>14}", "refs", "mode", "frame1_ms", "drain_ms", "frames", "worst_ms");
    for n in [1000usize, 5000] {
        for budgeted in [false, true] {
            let (mut world, refs) = vm_cost_world(n, SRC, "VmCostScript");
            census_mode(&mut world, budgeted);
            for r in &refs {
                world.insert(*r, crate::OnCellLoadEvent);
            }
            let first = census_frame(&world);
            for r in &refs {
                world.remove::<crate::OnCellLoadEvent>(*r);
            }
            let (mut drain, mut frames, mut worst) = (first, 1, first);
            while world.try_resource::<crate::FragmentExecutionQueue>().is_some_and(|q| !q.is_empty()) && frames < 10_000 {
                let ms = census_frame(&world);
                drain += ms;
                worst = worst.max(ms);
                frames += 1;
            }
            let mode = if budgeted { "budgeted" } else { "unbudgeted" };
            eprintln!("{n:>8} {mode:>10} {first:>14.3} {drain:>14.3} {frames:>8} {worst:>14.3}");
        }
    }

    eprintln!("\nC. polling: RegisterForUpdate(interval), 60 fps frames, µs per frame (all VM systems)");
    eprintln!("{:>8} {:>10} {:>10} {:>14} {:>14}", "refs", "interval", "mode", "mean_us/frame", "max_us/frame");
    for (n, interval) in [(100usize, "1.0"), (1000, "1.0"), (1000, "0.1"), (5000, "1.0"), (5000, "0.1")] {
        for budgeted in [false, true] {
            let src = format!(
                "ScriptName VmPollScript extends ObjectReference\n\
                 Int Property Count Auto\n\
                 Event OnInit()\n RegisterForUpdate({interval})\n EndEvent\n\
                 Event OnUpdate()\n Count += 1\n EndEvent\n"
            );
            let (mut world, refs) = vm_cost_world(n, &src, "VmPollScript");
            census_mode(&mut world, budgeted);
            for r in &refs {
                world.insert(*r, crate::OnCellLoadEvent);
            }
            // Settle the start-up burst before measuring steady polling.
            for _ in 0..200 {
                census_frame(&world);
                if world.try_resource::<crate::FragmentExecutionQueue>().is_none_or(|q| q.is_empty()) {
                    break;
                }
            }
            let frames = 600;
            let (mut sum, mut max) = (0.0f64, 0.0f64);
            for _ in 0..frames {
                let us = census_frame(&world) * 1e3;
                sum += us;
                max = max.max(us);
            }
            let mode = if budgeted { "budgeted" } else { "unbudgeted" };
            eprintln!("{n:>8} {interval:>10} {mode:>10} {:>14.2} {max:>14.2}", sum / f64::from(frames));
        }
    }

    eprintln!("\nD. runaway handlers (100000-statement cap each), OnActivate");
    eprintln!("{:>8} {:>10} {:>14} {:>14} {:>8} {:>14}", "handlers", "mode", "frame1_ms", "total_ms", "frames", "worst_ms");
    for handlers in [1usize, 5] {
        for budgeted in [false, true] {
            let (mut world, refs) = vm_cost_world(handlers, RUNAWAY, "VmRunawayScript");
            census_mode(&mut world, budgeted);
            activate_all(&mut world, &refs);
            let first = census_frame(&world);
            for r in &refs {
                world.remove::<crate::ActivateEvent>(*r);
            }
            let (mut total, mut frames, mut worst) = (first, 1, first);
            while world.try_resource::<crate::FragmentExecutionQueue>().is_some_and(|q| !q.is_empty()) && frames < 10_000 {
                let ms = census_frame(&world);
                total += ms;
                worst = worst.max(ms);
                frames += 1;
            }
            let mode = if budgeted { "budgeted" } else { "unbudgeted" };
            eprintln!("{handlers:>8} {mode:>10} {first:>14.3} {total:>14.3} {frames:>8} {worst:>14.3}");
        }
    }
}

// ── The frame budget (#5763) ─────────────────────────────────────────────────

fn budgeted(world: &mut World, limit: usize) {
    world.insert_resource(crate::VmFrameBudget::with_limit(limit));
    world.init_resource::<crate::FragmentExecutionQueue>();
}

/// A new frame as the schedule sees it: the end-of-frame cleanup renews the
/// allowance, the continuation system runs what is waiting.
fn next_frame(world: &World) {
    crate::event_cleanup_system(world, 0.0);
    crate::fragment_continuation_system(world, 1.0 / 60.0);
}

fn count_of(world: &World, index: usize) -> Option<f32> {
    world
        .resource::<crate::QuestScriptVariables>()
        .get(QuestFormId(0x0002_0000 + index as u32), crate::QuestScriptVariables::property_key("Count"))
}

const RUNAWAY: &str = "ScriptName VmRunawayScript extends ObjectReference\n\
     Int Property Count Auto\n\
     Event OnActivate(ObjectReference akActionRef)\n\
       While Count >= 0\n Count += 1\n EndWhile\n\
     EndEvent\n";

fn activate_all(world: &mut World, refs: &[byroredux_core::ecs::EntityId]) {
    let player = world.resource::<PapyrusPlayerEntity>().0;
    for r in refs {
        world.insert(*r, crate::ActivateEvent { activator: player });
    }
}

/// A run the budget cuts short suspends between two statements and resumes at
/// the next one: the loop runs exactly its iterations, none twice, none lost.
#[test]
fn a_run_the_budget_cuts_short_resumes_where_it_stopped() {
    let (mut world, refs) = vm_cost_world(
        1,
        "ScriptName VmCountScript extends ObjectReference\n\
         Int Property Count Auto\n\
         Int Property Done Auto\n\
         Event OnActivate(ObjectReference akActionRef)\n\
           While Count < 500\n Count += 1\n EndWhile\n\
           Done = 1\n\
         EndEvent\n",
        "VmCountScript",
    );
    budgeted(&mut world, 120);
    activate_all(&mut world, &refs);
    crate::reference_script_event_system(&world, 0.0);
    let after_first = count_of(&world, 0).expect("the loop ran");
    assert!(after_first > 0.0 && after_first < 500.0, "cut short mid-loop: {after_first}");
    assert_eq!(world.resource::<crate::FragmentExecutionQueue>().len(), 1, "one continuation queued");
    assert_eq!(world.resource::<crate::VmFrameBudget>().used().yielded, 1);

    let mut frames = 0;
    while !world.resource::<crate::FragmentExecutionQueue>().is_empty() {
        next_frame(&world);
        frames += 1;
        assert!(frames < 200, "the run never finished");
    }
    assert_eq!(count_of(&world, 0), Some(500.0), "every iteration ran exactly once");
    let done = world
        .resource::<crate::QuestScriptVariables>()
        .get(QuestFormId(0x0002_0000), crate::QuestScriptVariables::property_key("Done"));
    assert_eq!(done, Some(1.0), "the statement after the loop ran");
    assert!(frames > 2, "spread over frames, not finished in one: {frames}");
}

/// Several runaway handlers raised in one frame spend the frame's allowance
/// once between them: the first yields, the rest queue unstarted, and no frame
/// afterwards spends more than the allowance either. Each run is then killed at
/// the per-run cap, counted across its yields.
#[test]
fn several_runaway_handlers_stay_inside_the_frame_budget() {
    const LIMIT: usize = 1_000;
    const RUNS: usize = 5;
    let (mut world, refs) = vm_cost_world(RUNS, RUNAWAY, "VmRunawayScript");
    budgeted(&mut world, LIMIT);
    activate_all(&mut world, &refs);
    crate::reference_script_event_system(&world, 0.0);

    let used = world.resource::<crate::VmFrameBudget>().used();
    assert!(
        used.spent >= LIMIT && used.spent <= LIMIT + 4,
        "the frame spent the allowance and no more (one statement of overshoot at most): {}",
        used.spent
    );
    assert_eq!((used.yielded, used.deferred), (1, RUNS as u32 - 1), "one cut short, the rest not started");
    assert_eq!(world.resource::<crate::FragmentExecutionQueue>().len(), RUNS);

    // Frame after frame: never over the allowance; every run dies at the cap.
    let mut frames = 0;
    let mut worst = 0;
    while !world.resource::<crate::FragmentExecutionQueue>().is_empty() {
        crate::event_cleanup_system(&world, 0.0);
        crate::fragment_continuation_system(&world, 1.0 / 60.0);
        worst = worst.max(world.resource::<crate::VmFrameBudget>().used().spent);
        frames += 1;
        assert!(frames < 2_000, "the runaway handlers never ended");
    }
    assert!(worst <= LIMIT + RUNS * RUN_START_FUZZ, "a frame went over the allowance: {worst}");
    // 100 000 statements a run, two per loop iteration, whatever the slicing.
    for index in 0..RUNS {
        let count = count_of(&world, index).expect("each ran");
        assert!(
            (45_000.0..=50_001.0).contains(&count),
            "run {index} was killed at the cumulative cap, not per slice: {count}"
        );
    }
}

/// Slack for the run-start cost the first statement of each resumed slice
/// pays before the allowance is checked.
const RUN_START_FUZZ: usize = crate::fragment::RUN_START_COST + 4;

/// A wait suspends with no carried steps (the count starts over, as it always
/// did); a yield carries them.
#[test]
fn only_a_yield_carries_the_statement_count() {
    let (mut world, refs) = vm_cost_world(
        1,
        "ScriptName VmWaitScript extends ObjectReference\n\
         Int Property Count Auto\n\
         Event OnActivate(ObjectReference akActionRef)\n\
           Count += 1\n Utility.Wait(1.0)\n Count += 1\n\
         EndEvent\n",
        "VmWaitScript",
    );
    let carried = |world: &World| {
        let queue = world.resource::<crate::FragmentExecutionQueue>();
        let pending = queue.pending.first().cloned().expect("a continuation");
        match pending.effects.first() {
            Some(Effect::Interpret { resume: Some(resume), .. }) => resume.carried_steps,
            other => panic!("not a resumed run: {other:?}"),
        }
    };
    budgeted(&mut world, 100_000);
    activate_all(&mut world, &refs);
    crate::reference_script_event_system(&world, 0.0);
    assert_eq!(carried(&world), 0, "a wait starts the count afresh");

    // The same handler, the budget gone after its first statement.
    let (mut world, refs) = vm_cost_world(
        1,
        "ScriptName VmWaitScript extends ObjectReference\n\
         Int Property Count Auto\n\
         Event OnActivate(ObjectReference akActionRef)\n\
           Count += 1\n Count += 1\n Count += 1\n\
         EndEvent\n",
        "VmWaitScript",
    );
    budgeted(&mut world, crate::fragment::RUN_START_COST + 1);
    activate_all(&mut world, &refs);
    crate::reference_script_event_system(&world, 0.0);
    assert!(carried(&world) > 0, "a yield carries what it has used");
}

/// A function called for its value cannot hand a value back across a
/// suspension, and a frame with an array local cannot be stored: the run is
/// not cut there. It finishes in the frame it started and is charged for it,
/// so the work behind it defers.
#[test]
fn a_run_that_cannot_suspend_finishes_and_is_charged() {
    let (mut world, refs) = vm_cost_world(
        2,
        "ScriptName VmHeavyScript extends ObjectReference\n\
         Int Property Count Auto\n\
         Int Function Heavy()\n\
           Int i = 0\n While i < 300\n i += 1\n EndWhile\n Return i\n\
         EndFunction\n\
         Event OnActivate(ObjectReference akActionRef)\n\
           Count = Heavy()\n\
         EndEvent\n",
        "VmHeavyScript",
    );
    budgeted(&mut world, crate::fragment::RUN_START_COST + 20);
    activate_all(&mut world, &refs);
    crate::reference_script_event_system(&world, 0.0);
    assert_eq!(count_of(&world, 0), Some(300.0), "the value call ran whole");
    let used = world.resource::<crate::VmFrameBudget>().used();
    assert!(used.spent > 300, "and was charged for it: {}", used.spent);
    assert_eq!(used.yielded, 0, "never suspended");
    assert_eq!(used.deferred, 1, "the second reference waits for the next frame");
    assert_eq!(count_of(&world, 1), None);
    next_frame(&world);
    assert_eq!(count_of(&world, 1), Some(300.0), "and runs on it");
}

/// A reference whose handler cannot start for want of budget is not lost: the
/// event it was raised for is delivered on a later frame, once.
#[test]
fn deferred_deliveries_run_once_on_a_later_frame() {
    let (mut world, refs) = vm_cost_world(
        50,
        "ScriptName VmBurstScript extends ObjectReference\n\
         Int Property Count Auto\n\
         Event OnLoad()\n Count += 1\n EndEvent\n",
        "VmBurstScript",
    );
    budgeted(&mut world, 400);
    for r in &refs {
        world.insert(*r, crate::OnCellLoadEvent);
    }
    crate::reference_script_event_system(&world, 0.0);
    let ran = |world: &World| (0..50).filter(|i| count_of(world, *i).is_some()).count();
    let first = ran(&world);
    assert!(first > 0 && first < 50, "a slice of the burst ran: {first}");
    for r in &refs {
        world.remove::<crate::OnCellLoadEvent>(*r);
    }
    let mut frames = 0;
    while !world.resource::<crate::FragmentExecutionQueue>().is_empty() {
        next_frame(&world);
        frames += 1;
        assert!(frames < 100);
    }
    assert!(frames > 1, "spread over frames");
    for index in 0..50 {
        // OnInit, OnCellAttach, OnLoad and OnCellLoad are delivered for a load;
        // the script handles OnLoad (OnCellLoad is its twin), once each.
        assert!(count_of(&world, index).is_some_and(|count| count >= 1.0), "reference {index} never ran");
    }
}

// ── The reference-script index (#5763) ───────────────────────────────────────

#[test]
fn the_reference_script_index_follows_attach_and_unload() {
    const SRC: &str = "ScriptName VmIndexScript extends ObjectReference\n\
         Int Property Count Auto\n\
         Event OnLoad()\n Count += 1\n EndEvent\n";
    let (mut world, refs) = vm_cost_world(3, SRC, "VmIndexScript");
    let form = |i: usize| 0x0002_0000 + i as u32;
    for (i, r) in refs.iter().enumerate() {
        assert_eq!(crate::ReferenceScriptIndex::lookup(&world, form(i)), Some(*r));
    }
    assert_eq!(crate::ReferenceScriptIndex::lookup(&world, 0x00FF_FFFF), None, "never attached");

    // The cell unloads: the entity goes, and the reference reads as not loaded
    // with no map maintenance at all.
    world.despawn(refs[1]);
    assert_eq!(crate::ReferenceScriptIndex::lookup(&world, form(1)), None);
    assert_eq!(crate::ReferenceScriptIndex::lookup(&world, form(0)), Some(refs[0]), "the others stand");

    // It reloads as a new entity with the same FormID and the script attached
    // again: the next lookup finds that one.
    let script = parsed_source(SRC);
    let (events, scripts) = crate::check_reference_script(script, &crate::DELIVERED_EVENTS, None).expect("the VM runs it");
    let reloaded = world.spawn();
    let id = world.resource_mut::<FormIdPool>().intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(form(1)),
    });
    world.insert(reloaded, FormIdComponent(id));
    let instance = ScriptInstance { name: "VmIndexScript".into(), status: 0, properties: Vec::new() };
    crate::attach_reference_script(&mut world, reloaded, &instance, events, scripts);
    assert_eq!(crate::ReferenceScriptIndex::lookup(&world, form(1)), Some(reloaded));
}

/// A reference whose `FormIdComponent` lands after its scripts attach is
/// still found: the FormID is read when the map is rebuilt, not at attach.
#[test]
fn the_index_reads_the_formid_when_it_is_built_not_when_a_script_attaches() {
    let script = parsed_source(
        "ScriptName VmLateScript extends ObjectReference\n Event OnLoad()\n EndEvent\n",
    );
    let (events, scripts) = crate::check_reference_script(script, &crate::DELIVERED_EVENTS, None).expect("the VM runs it");
    let mut world = World::new();
    crate::register(&mut world);
    world.register::<FormIdComponent>();
    world.insert_resource(FormIdPool::default());
    let entity = world.spawn();
    let instance = ScriptInstance { name: "VmLateScript".into(), status: 0, properties: Vec::new() };
    crate::attach_reference_script(&mut world, entity, &instance, events, scripts);
    let id = world.resource_mut::<FormIdPool>().intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(0x0003_0001),
    });
    world.insert(entity, FormIdComponent(id));
    assert_eq!(crate::ReferenceScriptIndex::lookup(&world, 0x0003_0001), Some(entity));
}

/// A static native on an engine builtin class — `InputEnableLayer.
/// GetOrCreate(3)`, the FO4+/SF player-controls idiom — must be *routed*
/// through native classification, not fail the whole check as `unknown
/// name inputenablelayer`. The library stub being an engine class (native
/// functions, no bodies) is exactly what says "this names the class, not
/// a value". `GetOrCreate` itself has no effect-table entry yet, so the
/// honest outcome is a decline naming the native — which is what drives
/// the next primitive — instead of a misleading unknown-identifier error
/// that reads as a broken script.
#[test]
fn static_native_on_an_engine_class_names_the_native_not_an_unknown() {
    let input_enable_layer = parsed_source(
        "ScriptName InputEnableLayer extends Form\n\
         Function GetOrCreate(Int aiLayersToReserve = 1) Native\n",
    );
    let script = parsed_source(
        "ScriptName QF_VmStaticNative extends Quest\n\
         Function Fragment_1()\n\
           InputEnableLayer.GetOrCreate(3)\n\
         EndFunction\n",
    );
    let scripts: HashMap<String, Arc<Script>> =
        [("inputenablelayer".to_owned(), input_enable_layer)].into();
    let library = Installed(&scripts);
    assert_eq!(
        check_function_reason(Arc::clone(&script), "Fragment_1", Some(&library)).err(),
        Some("static native getorcreate/1".to_owned()),
        "the decline must name the unmodeled native, not report the class \
         itself as an unknown identifier"
    );
}

/// The routing must not capture an identifier that names a VALUE: a
/// property of the fragment's own script keeps its instance-call
/// semantics even when a same-named engine class exists in the library.
#[test]
fn a_value_named_like_an_engine_class_still_resolves_as_a_value() {
    let message_stub = parsed_source(
        "ScriptName Message extends Form\n\
         Function Show(Float af1 = 0, Float af2 = 0) Native\n",
    );
    let script = parsed_source(
        "ScriptName QF_VmShadowedStatic extends Quest\n\
         Message Property MyMessage Auto\n\
         Function Fragment_1()\n\
           MyMessage.Show(1.0)\n\
         EndFunction\n",
    );
    let scripts: HashMap<String, Arc<Script>> = [("message".to_owned(), message_stub)].into();
    let library = Installed(&scripts);
    assert_eq!(
        check_function_reason(Arc::clone(&script), "Fragment_1", Some(&library)).err(),
        None,
        "MyMessage is a property — the call lowers through the instance path"
    );
}

/// Distance events: `RegisterForDistanceLessThanEvent` delivers a single
/// `OnDistanceLessThan` once the pair is close, an alias counts once filled,
/// a re-registration only updates the distance, and
/// `UnregisterForDistanceEvents` cancels it.
#[test]
fn distance_events_fire_once_when_the_pair_is_close() {
    use byroredux_core::ecs::components::GlobalTransform;
    use byroredux_core::math::{Quat, Vec3};
    const FAR: u32 = 0x0001_0F50;
    const NEAR: u32 = 0x0001_0F51;
    let script = parsed_source(
        "ScriptName QF_VmDistance extends Quest\n\
         ObjectReference Property A Auto\n\
         ObjectReference Property B Auto\n\
         ObjectReference Property C Auto\n\
         Int Fired\n\
         Function Fragment_1()\n\
           RegisterForDistanceLessThanEvent(A, B, 50.0)\n\
           RegisterForDistanceLessThanEvent(B, A, 500.0)\n\
           RegisterForDistanceLessThanEvent(A, C, 50.0)\n\
           UnregisterForDistanceEvents(A, C)\n\
         EndFunction\n\
         Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)\n\
           Fired += 1\n\
           If afDistance < 300.0 && akObj1 == A\n Self.SetStage(20)\n EndIf\n\
         EndEvent\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.insert_resource(crate::QuestScriptVariables::default());
    world.register::<FormIdComponent>();
    world.register::<GlobalTransform>();
    let mut pool = FormIdPool::default();
    let mut spawn_at = |world: &mut World, local: u32, x: f32| {
        let id = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("Skyrim.esm"),
            local: LocalFormId(local),
        });
        let entity = world.spawn();
        world.insert(entity, FormIdComponent(id));
        world.insert(entity, GlobalTransform::new(Vec3::new(x, 0.0, 0.0), Quat::IDENTITY, 1.0));
        entity
    };
    let a = spawn_at(&mut world, FAR, 0.0);
    let b = spawn_at(&mut world, NEAR, 400.0);
    world.insert_resource(pool);
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        let mut vmad = vmad();
        for (name, form) in [("A", FAR), ("B", NEAR), ("C", 0x0001_0F52)] {
            vmad.scripts[0].properties.push(ScriptProperty {
                name: name.into(),
                status: 1,
                value: PropertyValue::Object {
                    form_id: form,
                    alias: -1,
                },
            });
        }
        fragments.insert_vmad(Q, vmad);
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    // (B, A) re-registered the pair at 500: one watch, not two.
    assert_eq!(world.resource::<crate::DistanceWatches>().watches().len(), 1);
    assert_eq!(world.resource::<crate::DistanceWatches>().watches()[0].distance, 500.0);
    crate::distance_event_system(&world, 0.0);
    // 400 < 500: delivered once, with the pair in its first order.
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 10, "400 units is not under 300");
    assert!(world.resource::<crate::DistanceWatches>().watches().is_empty(), "single event");
    let fired = world
        .resource::<crate::QuestScriptVariables>()
        .get(Q, crate::QuestScriptVariables::property_key("Fired"));
    assert_eq!(fired, Some(1.0));
    crate::distance_event_system(&world, 0.0);
    assert_eq!(
        world
            .resource::<crate::QuestScriptVariables>()
            .get(Q, crate::QuestScriptVariables::property_key("Fired")),
        Some(1.0)
    );
    let _ = (a, b);
}

/// An array a script variable holds is one shared value within a run and is
/// written back whole when the run ends: a later fragment sees the elements
/// the earlier one added, structs included.
#[test]
fn script_variable_arrays_persist_between_runs() {
    let script = parsed_source(
        "ScriptName QF_VmHeld extends Quest\n\
         Struct Entry\n Int Id\n EndStruct\n\
         ObjectReference Property Marker Auto\n\
         Entry[] Rows\n\
         Int[] Counts\n\
         Function Fragment_1()\n\
           Counts = new Int[0]\n\
           Counts.Add(4)\n\
           Counts.Add(9)\n\
           Rows = new Entry[0]\n\
           Entry e = new Entry\n\
           e.Id = 7\n\
           Rows.Add(e)\n\
           Rows[0].Id = 8\n\
         EndFunction\n\
         Function Fragment_2()\n\
           If Counts.Length == 2 && Counts[1] == 9 && Rows.Length == 1 && Rows[0].Id == 8\n\
             Counts.Remove(0)\n\
             Marker.Enable()\n\
           EndIf\n\
         EndFunction\n\
         Function Fragment_3()\n\
           If Counts.Length == 1 && Counts[0] == 9\n\
             Self.SetStage(30)\n\
           EndIf\n\
         EndFunction\n",
    );
    for function in ["Fragment_1", "Fragment_2", "Fragment_3"] {
        assert_eq!(check_function_reason(Arc::clone(&script), function, None).err(), None, "{function}");
    }
    let world = world_with_fragment(script);
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        for (stage, function) in [(20, "Fragment_1"), (21, "Fragment_2"), (22, "Fragment_3")] {
            fragments.insert(
                Q,
                stage,
                vec![Effect::Interpret {
                    script: "QF_VmHeld".into(),
                    function: function.into(),
                    args: Vec::new(),
                    resume: None,
                }],
            );
        }
    }
    for stage in [20, 21, 22] {
        world.resource_mut::<QuestStageState>().set_stage(Q, stage);
        crate::fragment::quest_fragment_dispatch_system(&world);
    }
    assert!(crate::fragment::reference_enabled(&world, MARKER), "the second run read the first's arrays");
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 30, "and the third read the second's Remove");
}

/// Runs `body` as Fragment_1 (which ends with `Marker.Enable()`) and reports
/// whether the marker was enabled — false means the run was stopped first.
fn run_body_enables_marker(body: &str) -> bool {
    let script = parsed_source(&format!(
        "ScriptName QF_VmCap extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n\
         {body}\
           Marker.Enable()\n\
         EndFunction\n",
    ));
    let world = world_with_fragment(script);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    crate::fragment::reference_enabled(&world, MARKER)
}

/// `new T[n]` is capped at the SDK's `MAX_SCRIPT_ARRAY_ELEMENTS` — `n` is a
/// runtime value here, so `new Int[6000000000]` must stop the run, not
/// allocate (#5791).
#[test]
fn vm_array_new_is_capped() {
    assert!(
        !run_body_enables_marker("Int[] tooBig = new Int[600]\n"),
        "a 600-element array stops the run"
    );
    assert!(
        run_body_enables_marker("Int[] atCap = new Int[512]\n"),
        "512 is the documented ceiling and stays legal"
    );
}

/// `Array.Add(x, count)` is capped on `len + count`, so one call cannot push
/// billions of elements (#5791).
#[test]
fn vm_array_add_count_is_capped() {
    assert!(
        !run_body_enables_marker("Int[] a = new Int[0]\n  a.Add(1, 600)\n"),
        "adding 600 at once stops the run"
    );
    assert!(
        run_body_enables_marker("Int[] a = new Int[510]\n  a.Add(1, 2)\n"),
        "512 total stays legal"
    );
}

/// `Array.Remove(index, count)` with a count that would overflow `index +
/// count` clamps to the array end instead of panicking (#5981).
#[test]
fn vm_array_remove_huge_count_clamps() {
    assert!(
        run_body_enables_marker("Int[] a = new Int[3]\n  a.Remove(1, 2147483647)\n"),
        "an overflowing count removes through the end and the run continues"
    );
}

/// A Float operand sends `%` down the truncating branch; `i32::MIN % -1`
/// there must wrap like the Int branch, not abort the process (#5982).
#[test]
fn vm_float_modulo_overflow_wraps() {
    let got = arithmetic(BinaryOp::Mod, &Value::Float(-2147483648.0), &Value::Float(-1.0)).unwrap();
    assert!(matches!(got, Value::Int(0)), "i32::MIN % -1 wraps to 0");
    let ok = arithmetic(BinaryOp::Mod, &Value::Float(7.0), &Value::Float(3.0)).unwrap();
    assert!(matches!(ok, Value::Int(1)), "ordinary truncating modulo is unchanged");
}

/// A struct that contains itself cannot be stored: the run-end write-back
/// refuses it instead of recursing until the stack overflows (#5983), while
/// ordinary two-level nesting still stores.
#[test]
fn vm_cyclic_struct_is_not_stored() {
    let cyclic = Rc::new(RefCell::new(vec![("m".to_owned(), Value::Int(1))]));
    cyclic.borrow_mut()[0].1 = Value::Struct(Rc::clone(&cyclic));
    assert!(to_stored(&Value::Struct(cyclic)).is_none(), "a self-containing struct is refused");

    let inner = Value::Struct(Rc::new(RefCell::new(vec![("x".to_owned(), Value::Int(2))])));
    let outer = Value::Struct(Rc::new(RefCell::new(vec![("in".to_owned(), inner)])));
    assert!(to_stored(&outer).is_some(), "a struct holding a struct stores");
}

/// `Array.Insert` past the ceiling is refused like `Add` (#5791).
#[test]
fn vm_array_insert_is_capped() {
    assert!(
        !run_body_enables_marker(
            "Int[] a = new Int[0]\n\
             Int i = 0\n\
             While i < 512\n\
               a.Add(7)\n\
               i = i + 1\n\
             EndWhile\n\
             a.Insert(0, 5)\n"
        ),
        "inserting into a full 512-element array stops the run"
    );
}

/// String concatenation is capped at the SDK's `MAX_SCRIPT_STRING_BYTES`:
/// `s = s + s` doubles per statement and would reach terabytes long before
/// `MAX_STEPS` bites (#5791).
#[test]
fn vm_string_concat_is_capped() {
    assert!(
        !run_body_enables_marker(
            "String s = \"xxxx\"\n\
             Int i = 0\n\
             While i < 13\n\
               s = s + s\n\
               i = i + 1\n\
             EndWhile\n"
        ),
        "doubling past 4096 bytes stops the run"
    );
    assert!(
        run_body_enables_marker("String s = \"ab\" + \"cd\"\n"),
        "short concatenations stay legal"
    );
}

/// A script-filled `RefCollectionAlias`: `AddRef` / `AddArray` / `RemoveRef`
/// / `RemoveAll` change it, `GetCount` / `GetAt` / `Find` read it, across
/// runs; a collection the quest fills by conditions is refused.
#[test]
fn script_filled_collection_aliases() {
    use byroredux_plugin::esm::records::{QuestAlias, QustRecord};
    const FIRST: u32 = 0x0001_0F60;
    const SECOND: u32 = 0x0001_0F61;
    let script = parsed_source(
        "ScriptName QF_VmCollection extends Quest\n\
         RefCollectionAlias Property Crowd Auto\n\
         RefCollectionAlias Property Filled Auto\n\
         ObjectReference Property A Auto\n\
         ObjectReference Property B Auto\n\
         Function Fragment_1()\n\
           Crowd.AddRef(A)\n\
           Crowd.AddRef(B)\n\
           Crowd.AddRef(A)\n\
         EndFunction\n\
         Function Fragment_2()\n\
           If Crowd.GetCount() == 2 && Crowd.Find(B) == 1 && Crowd.GetAt(0) == A\n\
             ObjectReference[] all = Crowd.GetArray()\n\
             If all.Length == 2\n\
               Crowd.EvaluateAll()\n\
               Crowd.DisableAll()\n\
               Crowd.RemoveRef(A)\n\
               Self.SetStage(30)\n\
             EndIf\n\
           EndIf\n\
         EndFunction\n\
         Function Fragment_3()\n\
           If Crowd.GetCount() == 1 && Crowd.GetAt(0) == B\n\
             Crowd.RemoveAll()\n\
           EndIf\n\
           Filled.GetCount()\n\
         EndFunction\n",
    );
    for function in ["Fragment_1", "Fragment_2", "Fragment_3"] {
        assert_eq!(check_function_reason(Arc::clone(&script), function, None).err(), None, "{function}");
    }
    let mut world = world_with_fragment(script);
    world.insert_resource(crate::QuestScriptVariables::default());
    world.register::<FormIdComponent>();
    world.register::<crate::EvaluatePackageRequest>();
    let mut pool = FormIdPool::default();
    let member_entities: Vec<_> = [FIRST, SECOND]
        .into_iter()
        .map(|form| {
            let id = pool.intern(FormIdPair {
                plugin: PluginId::from_filename("Skyrim.esm"),
                local: LocalFormId(form),
            });
            let entity = world.spawn();
            world.insert(entity, FormIdComponent(id));
            entity
        })
        .collect();
    world.insert_resource(pool);
    let record = QustRecord {
        form_id: Q.0,
        aliases: vec![
            QuestAlias {
                alias_id: 3,
                is_collection: true,
                ..Default::default()
            },
            QuestAlias {
                alias_id: 4,
                is_collection: true,
                match_conditions: byroredux_plugin::esm::records::condition::ConditionList::from(
                    [Default::default()],
                ),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    crate::scene::install_scene_quest_aliases(&mut world, [record]);
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        let mut vmad = vmad();
        let alias = |name: &str, id: i16| ScriptProperty {
            name: name.into(),
            status: 1,
            value: PropertyValue::Object {
                form_id: Q.0,
                alias: id,
            },
        };
        let object = |name: &str, form: u32| ScriptProperty {
            name: name.into(),
            status: 1,
            value: PropertyValue::Object {
                form_id: form,
                alias: -1,
            },
        };
        vmad.scripts[0].properties.extend([alias("Crowd", 3), alias("Filled", 4), object("A", FIRST), object("B", SECOND)]);
        fragments.insert_vmad(Q, vmad);
        for (stage, function) in [(20, "Fragment_1"), (21, "Fragment_2"), (22, "Fragment_3")] {
            fragments.insert(
                Q,
                stage,
                vec![Effect::Interpret {
                    script: "QF_VmCollection".into(),
                    function: function.into(),
                    args: Vec::new(),
                    resume: None,
                }],
            );
        }
    }
    for stage in [20, 21] {
        world.resource_mut::<QuestStageState>().set_stage(Q, stage);
        crate::fragment::quest_fragment_dispatch_system(&world);
    }
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 30, "the count, find and get-at read");
    assert_eq!(world.resource::<crate::AliasCollections>().members((Q.0, 3)), [SECOND]);
    assert!(!crate::fragment::reference_enabled(&world, FIRST) && !crate::fragment::reference_enabled(&world, SECOND), "DisableAll");
    assert!(
        member_entities.iter().all(|entity| world.get::<crate::EvaluatePackageRequest>(*entity).is_some()),
        "EvaluateAll asks every member to re-evaluate its package"
    );
    world.resource_mut::<QuestStageState>().set_stage(Q, 22);
    crate::fragment::quest_fragment_dispatch_system(&world);
    // RemoveAll emptied it; the conditions-filled one was refused, not read as
    // empty (the run stopped there, after RemoveAll).
    assert!(world.resource::<crate::AliasCollections>().members((Q.0, 3)).is_empty());
}

/// `HasLocRefType` reads the reference's XLRT tags.
#[test]
fn has_loc_ref_type_reads_the_placement_tags() {
    const TAGGED: u32 = 0x0001_0F70;
    const BOSS: u32 = 0x0001_0F71;
    const OTHER: u32 = 0x0001_0F72;
    let script = parsed_source(
        "ScriptName QF_VmRefType extends Quest\n\
         ObjectReference Property Subject Auto\n\
         Form Property BossType Auto\n\
         Form Property OtherType Auto\n\
         Function Fragment_1()\n\
           If Subject.HasLocRefType(BossType as LocationRefType) && !Subject.HasLocRefType(OtherType as LocationRefType)\n\
             Self.SetStage(20)\n\
           EndIf\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(TAGGED),
    });
    world.insert_resource(pool);
    let subject = world.spawn();
    world.insert(subject, FormIdComponent(id));
    world.insert(
        subject,
        crate::scene::SceneAliasCandidate {
            reference_form_id: TAGGED,
            location_ref_types: vec![BOSS],
            ..Default::default()
        },
    );
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        let mut vmad = vmad();
        for (name, form) in [("Subject", TAGGED), ("BossType", BOSS), ("OtherType", OTHER)] {
            vmad.scripts[0].properties.push(ScriptProperty {
                name: name.into(),
                status: 1,
                value: PropertyValue::Object {
                    form_id: form,
                    alias: -1,
                },
            });
        }
        fragments.insert_vmad(Q, vmad);
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
}

/// Custom events: a registered script's `(akSender, akArgs)` handler runs from
/// the continuation queue after the sender's run, with the array it sent; an
/// unregistered one does not. Remote events relay what a reference received.
#[test]
fn custom_and_remote_events_reach_registered_scripts() {
    const DOOR: u32 = 0x0001_0F80;
    let script = parsed_source(
        "ScriptName QF_VmEvents extends Quest\n\
         CustomEvent Ping\n\
         ObjectReference Property Marker Auto\n\
         ObjectReference Property Door Auto\n\
         Int Pings\n\
         Function Fragment_1()\n\
           RegisterForCustomEvent(Self as ScriptObject, \"qf_vmevents_Ping\")\n\
           RegisterForRemoteEvent(Door, \"OnActivate\")\n\
           Var[] args = new Var[2]\n\
           args[0] = 5\n\
           args[1] = 2.5\n\
           SendCustomEvent(\"qf_vmevents_Ping\", args)\n\
           Pings = 100\n\
         EndFunction\n\
         Event QF_VmEvents_Ping(QF_VmEvents akSender, Var[] akArgs)\n\
           Pings += 1\n\
           If akArgs.Length == 2 && (akArgs[0] as Int) == 5\n Marker.Enable()\n EndIf\n\
         EndEvent\n\
         Event ObjectReference_OnActivate(ObjectReference akSender, ObjectReference akActionRef)\n\
           Pings += 10\n\
         EndEvent\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.insert_resource(crate::QuestScriptVariables::default());
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(DOOR),
    });
    world.insert_resource(pool);
    let door = world.spawn();
    world.insert(door, FormIdComponent(id));
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        let mut vmad = vmad();
        vmad.scripts[0].properties.push(ScriptProperty {
            name: "Door".into(),
            status: 1,
            value: PropertyValue::Object {
                form_id: DOOR,
                alias: -1,
            },
        });
        fragments.insert_vmad(Q, vmad);
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    let pings = |world: &World| {
        world
            .resource::<crate::QuestScriptVariables>()
            .get(Q, crate::QuestScriptVariables::property_key("Pings"))
    };
    // The sender's run finished (Pings = 100) before the receiver ran.
    assert_eq!(pings(&world), Some(100.0));
    assert!(!crate::fragment::reference_enabled(&world, MARKER));
    crate::fragment::fragment_continuation_system(&world, 0.0);
    assert_eq!(pings(&world), Some(101.0), "the custom handler ran once, with the sent array");
    assert!(crate::fragment::reference_enabled(&world, MARKER));
    // The door is activated: the remote registrant hears it.
    world.insert(door, crate::ActivateEvent { activator: door });
    crate::reference_script_event_system(&world, 0.0);
    crate::fragment::fragment_continuation_system(&world, 0.0);
    assert_eq!(pings(&world), Some(111.0));
}

/// `RegisterForAnimationEvent` registers the script for an event of a reference
/// with a behavior graph (true; false without one), and the graph's raised events
/// deliver `OnAnimationEvent(akSource, asEventName)` to it: the clip trigger's
/// event does, an event it did not register for does not, and after an
/// `UnregisterForAnimationEvent` nothing does.
#[test]
fn animation_events_reach_registered_scripts() {
    use byroredux_core::animation::AnimationPlayer;
    use byroredux_hkx::{BehaviorGraph, ClipMode, ClipTrigger, State, StateClip, StateMachine, Transition};
    const OTHER: u32 = 0x0001_0F81;
    let script = parsed_source(
        "ScriptName QF_VmAnimEvents extends Quest\n\
         ObjectReference Property Marker Auto\n\
         ObjectReference Property Bare Auto\n\
         Int Heard\n\
         Int Lost\n\
         Int Registered\n\
         Function Fragment_1()\n\
           If RegisterForAnimationEvent(Marker, \"Done\") && !RegisterForAnimationEvent(Bare, \"Done\")\n\
             Registered = 1\n\
           EndIf\n\
         EndFunction\n\
         Function Fragment_2()\n\
           UnregisterForAnimationEvent(Marker, \"done\")\n\
         EndFunction\n\
         Event OnAnimationEvent(ObjectReference akSource, String asEventName)\n\
           If akSource == Marker && asEventName == \"Done\"\n\
             Heard += 1\n\
           EndIf\n\
         EndEvent\n\
         Event OnAnimationEventUnregistered(ObjectReference akSource, String asEventName)\n\
           If akSource == Marker && asEventName == \"Done\"\n\
             Lost += 1\n\
           EndIf\n\
         EndEvent\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.insert_resource(crate::QuestScriptVariables::default());
    world.register::<FormIdComponent>();
    world.register::<AnimationPlayer>();
    world.register::<crate::ObjectAnimationGraph>();
    let mut pool = FormIdPool::default();
    let marker_id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(MARKER) });
    let bare_id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(OTHER) });
    world.insert_resource(pool);
    let marker = world.spawn();
    world.insert(marker, FormIdComponent(marker_id));
    let bare = world.spawn();
    world.insert(bare, FormIdComponent(bare_id));
    let clip = StateClip {
        animation: "a/go.hkx".into(),
        mode: ClipMode::SinglePlay,
        playback_speed: 1.0,
        start_time: 0.0,
        user_fraction: 0.0,
        triggers: vec![ClipTrigger { local_time: 0.5, event: 1, relative_to_end: false, annotation: false }],
            playback_speed_variable: None,
    };
    let graph = BehaviorGraph {
        variables: Vec::new(),
        events: vec!["Go".into(), "Done".into()],
        machine: Some(StateMachine {
            start_state: 0,
            start_state_variable: None,
            states: vec![
                State { id: 0, name: "Idle".into(), transitions: vec![Transition { event: 0, to_state: 1, priority: 0, flags: 0, ..Default::default() }], clip: None, content: Default::default() },
                State { id: 1, name: "Going".into(), transitions: Vec::new(), clip: Some(clip), content: Default::default() },
            ],
            wildcard: Vec::new(),
        }),
    };
    let mut clips = std::collections::HashMap::new();
    clips.insert(("a/go.hkx".to_owned(), false), crate::GraphClip { handle: 4, duration: 1.0 });
    world.insert(
        marker,
        crate::ObjectAnimationGraph::new(Arc::new(crate::LoadedGraph { graph, clips, sequences: Default::default(), lazy: None }), marker).unwrap(),
    );
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        let mut vmad = vmad();
        for (name, form_id) in [("Marker", MARKER), ("Bare", OTHER)] {
            vmad.scripts[0].properties.push(ScriptProperty {
                name: name.into(),
                status: 1,
                value: PropertyValue::Object { form_id, alias: -1 },
            });
        }
        fragments.insert_vmad(Q, vmad);
        fragments.insert(
            Q,
            20,
            vec![Effect::Interpret {
                script: "QF_VmAnimEvents".into(),
                function: "Fragment_2".into(),
                args: Vec::new(),
                resume: None,
            }],
        );
    }
    let variable = |world: &World, name: &str| {
        world.resource::<crate::QuestScriptVariables>().get(Q, crate::QuestScriptVariables::property_key(name))
    };
    let tick = |world: &World, dt: f32| {
        crate::object_graph_step_system(world, dt);
        crate::animation_event_system(world, 0.0);
        crate::fragment::fragment_continuation_system(world, 0.0);
    };
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(variable(&world, "Registered"), Some(1.0), "true with a graph, false without");
    // `Go` raises no registered event; the clip's trigger raises `Done` 0.5 s in.
    assert!(crate::animation_graph::play_animation(&world, marker, "Go"));
    tick(&world, 0.4);
    assert_eq!(variable(&world, "Heard"), None, "Done is not due yet");
    tick(&world, 0.2);
    assert_eq!(variable(&world, "Heard"), Some(1.0), "Done reached OnAnimationEvent");
    // A `SoundPlay.<name>` notify plays the named descriptor from the object.
    assert!(world.resource::<crate::PendingSoundPlays>().0.is_empty());
    let mut sounds = byroredux_plugin::esm::records::EsmIndex::default();
    sounds.sounds.insert(
        0x0005_0001,
        byroredux_plugin::esm::records::SounRecord { form_id: 0x0005_0001, editor_id: "OBJClack".into(), ..Default::default() },
    );
    crate::install_sound_editor_ids(&mut world, &sounds);
    {
        let mut graphs = world.query_mut::<crate::ObjectAnimationGraph>().unwrap();
        graphs.get_mut(marker).unwrap().push_raised_for_test("soundplay.OBJClack");
    }
    crate::animation_event_system(&world, 0.0);
    assert_eq!(
        world.resource::<crate::PendingSoundPlays>().0,
        [crate::SoundPlay { sound: 0x0005_0001, source: MARKER }]
    );
    // The sender's cell unloads: the registration is dropped and the script told.
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<crate::AnimationEventRegistrations>().entries().len(), 1, "registered again");
    crate::raise_unload_events(&world, &[marker]);
    crate::fragment::fragment_continuation_system(&world, 0.0);
    assert!(world.resource::<crate::AnimationEventRegistrations>().entries().is_empty(), "dropped with the graph");
    assert_eq!(variable(&world, "Lost"), Some(1.0), "OnAnimationEventUnregistered ran with the event");
    // Unregistered (the call lowercases the name: matching ignores case), nothing more.
    world.resource_mut::<QuestStageState>().set_stage(Q, 20);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert!(world.resource::<crate::AnimationEventRegistrations>().entries().is_empty());
}

/// `DamageObject` walks a destructible object through its stages: the damage
/// accumulates under the reference's FormID, `GetCurrentDestructionStage` reads
/// the model damage stage (-1 undamaged), a Destroy stage sets the destroyed
/// flag and disables the reference, `ClearDestruction` resets all of it, and an
/// object whose base has no destructible data takes nothing.
#[test]
fn damage_object_walks_the_destruction_stages() {
    use byroredux_plugin::esm::cell::{DestructionData, DestructionStage};
    const BASE: u32 = 0x0001_0F90;
    let script = parsed_source(
        "ScriptName QF_VmDamage extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n\
           If Marker.GetCurrentDestructionStage() == -1 && !Marker.IsDestroyed()\n\
             Marker.DamageObject(60.0)\n\
             If Marker.GetCurrentDestructionStage() == 1\n\
               Marker.DamageObject(50)\n\
               If Marker.GetCurrentDestructionStage() == 2 && Marker.IsDestroyed()\n\
                 Self.SetStage(20)\n\
               EndIf\n\
             EndIf\n\
           EndIf\n\
         EndFunction\n\
         Function Fragment_2()\n\
           Marker.ClearDestruction()\n\
           Marker.SetDestroyed(False)\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_2", None).err(), None);
    let mut world = world_with_fragment(script);
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(MARKER) });
    world.insert_resource(pool);
    let marker = world.spawn();
    world.insert(marker, FormIdComponent(id));
    world.insert(marker, crate::scene::SceneAliasCandidate { base_form_id: BASE, ..Default::default() });
    let stage = |percent, model, flags| DestructionStage { health_percent: percent, index: model, model_damage_stage: model, flags };
    world.insert_resource(crate::destruction::DestructionTable(
        [(
            BASE,
            DestructionData {
                health: 100,
                stages: vec![stage(50, 1, 0), stage(0, 2, DestructionStage::DESTROY | DestructionStage::DISABLE)],
            },
        )]
        .into(),
    ));
    world.resource_mut::<crate::fragment::ReferenceEnableState>().set_enabled(MARKER, true);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
    assert!(crate::destruction::is_destroyed(&world, MARKER));
    assert!(!crate::fragment::reference_enabled(&world, MARKER), "the Disable flag disabled it");
    crate::destruction::clear_destruction(&world, MARKER);
    crate::destruction::set_destroyed(&world, MARKER, false);
    assert!(world.resource::<crate::destruction::DestructionStates>().0.is_empty());
    assert_eq!(crate::destruction::destruction_stage(&world, marker, MARKER), -1);
}

/// `Weapon.Fire(akSource, akAmmo)` records a launch for the host: the weapon, the
/// source reference and the ammo (None: 0, "the current ammo").
#[test]
fn weapon_fire_records_a_projectile_launch() {
    const WEAPON: u32 = 0x0001_42E3;
    const AMMO: u32 = 0x0002_36DD;
    let script = parsed_source(
        "ScriptName QF_VmFire extends Quest\n\
         Weapon Property Dart Auto\n\
         Ammo Property Bolt Auto\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n\
           Dart.Fire(Marker, Bolt)\n\
           Dart.Fire(Marker)\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let world = world_with_fragment(script);
    let property = |name: &str, form_id| ScriptProperty {
        name: name.into(),
        status: 1,
        value: PropertyValue::Object { form_id, alias: -1 },
    };
    world.resource_mut::<crate::QuestStageFragments>().insert_vmad(
        Q,
        ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "QF_VmFire".into(),
                status: 0,
                properties: vec![property("Dart", WEAPON), property("Bolt", AMMO), property("Marker", MARKER)],
            }],
        },
    );
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(
        world.resource::<crate::PendingProjectileLaunches>().0,
        [
            crate::ProjectileLaunch { source: MARKER, weapon: WEAPON, ammo: AMMO, aim_at: 0, harmless: false },
            crate::ProjectileLaunch { source: MARKER, weapon: WEAPON, ammo: 0, aim_at: 0, harmless: false },
        ]
    );
}

/// `CreateDetectionEvent(akOwner, aiSoundLevel)` is recorded at the object it is
/// called on (the level defaulting to 0).
#[test]
fn create_detection_event_is_recorded() {
    const OWNER: u32 = 0x0001_0F70;
    let script = parsed_source(
        "ScriptName QF_VmDetect extends Quest\n\
         ObjectReference Property Marker Auto\n\
         Actor Property Bob Auto\n\
         Function Fragment_1()\n\
           Marker.CreateDetectionEvent(Bob, 60)\n\
           Marker.CreateDetectionEvent(Bob)\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let world = world_with_fragment(script);
    let property = |name: &str, form_id| ScriptProperty {
        name: name.into(),
        status: 1,
        value: PropertyValue::Object { form_id, alias: -1 },
    };
    world.resource_mut::<crate::QuestStageFragments>().insert_vmad(
        Q,
        ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "QF_VmDetect".into(),
                status: 0,
                properties: vec![property("Marker", MARKER), property("Bob", OWNER)],
            }],
        },
    );
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(
        world.resource::<crate::PendingDetectionEvents>().0,
        [
            crate::DetectionEvent { source: MARKER, owner: OWNER, sound_level: 60 },
            crate::DetectionEvent { source: MARKER, owner: OWNER, sound_level: 0 },
        ]
    );
}

/// `ProcessTrapHit` lands the trap's damage as the target's `HitEvent`;
/// `Game.ShakeCamera` / `ShakeController` become the player's transient command
/// markers, clamped to 0..1 with the camera's 0 duration meaning the game
/// setting's 1.25 s.
#[test]
fn trap_hits_and_shakes_reach_their_consumers() {
    const TRAP: u32 = 0x0001_0F72;
    let script = parsed_source(
        "ScriptName QF_VmTrapHit extends Quest\n\
         ObjectReference Property Marker Auto\n\
         ObjectReference Property Trap Auto\n\
         Function Fragment_1()\n\
           Marker.ProcessTrapHit(Trap, 7.5, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 9, 0.0)\n\
           Game.ShakeCamera(None, 2.0)\n\
           Game.ShakeController(0.25, 3.0, 0.5)\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    let property = |name: &str, form_id| ScriptProperty {
        name: name.into(),
        status: 1,
        value: PropertyValue::Object { form_id, alias: -1 },
    };
    world.resource_mut::<crate::QuestStageFragments>().insert_vmad(
        Q,
        ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "QF_VmTrapHit".into(),
                status: 0,
                properties: vec![property("Marker", MARKER), property("Trap", TRAP)],
            }],
        },
    );
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let marker_id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(MARKER) });
    let trap_id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(TRAP) });
    world.insert_resource(pool);
    let marker = world.spawn();
    world.insert(marker, FormIdComponent(marker_id));
    let trap = world.spawn();
    world.insert(trap, FormIdComponent(trap_id));
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    // #5872 — the request is queued; the Update flush lands it as the HitEvent.
    assert!(world.query::<crate::HitEvent>().unwrap().get(marker).is_none());
    crate::pending_trap_hit_flush_system(&world, 0.0);
    assert!(world.resource::<crate::PendingTrapHits>().0.is_empty());
    let events = world.query::<crate::HitEvent>().unwrap();
    let hit = events.get(marker).expect("the target was hit");
    assert_eq!((hit.aggressor, hit.source, hit.damage), (trap, trap, 7.5));
    drop(events);
    let player = world.resource::<PapyrusPlayerEntity>().0;
    let camera = *world.get::<crate::papyrus_demo::CameraShakeCommand>(player).expect("camera shake");
    assert_eq!((camera.intensity, camera.duration_secs), (1.0, 1.25));
    let rumble = *world.get::<crate::papyrus_demo::ControllerRumbleCommand>(player).expect("rumble");
    assert_eq!((rumble.left, rumble.right, rumble.duration_secs), (0.25, 1.0, 0.5));
}

/// A call for effect on a `None` object is ignored and the script goes on (CK:
/// "the call is ignored and the error printed out"); a call whose value is used
/// has nothing to give and stops the run.
#[test]
fn a_call_on_none_is_ignored_for_effect_and_stops_for_a_value() {
    let script = parsed_source(
        "ScriptName QF_VmNone extends Quest\n\
         Sound Property Missing Auto\n\
         ObjectReference Property Marker Auto\n\
         Function Fragment_1()\n\
           Missing.Play(Marker)\n\
           Self.SetStage(20)\n\
         EndFunction\n\
         Function Fragment_2()\n\
           Int id = Missing.Play(Marker)\n\
           Self.SetStage(30)\n\
         EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let world = world_with_fragment(script);
    world.resource_mut::<crate::QuestStageFragments>().insert_vmad(
        Q,
        ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "QF_VmNone".into(),
                status: 0,
                properties: vec![ScriptProperty {
                    name: "Marker".into(),
                    status: 1,
                    value: PropertyValue::Object { form_id: MARKER, alias: -1 },
                }],
            }],
        },
    );
    world.resource_mut::<crate::QuestStageFragments>().insert(
        Q,
        15,
        vec![Effect::Interpret { script: "QF_VmNone".into(), function: "Fragment_2".into(), args: Vec::new(), resume: None }],
    );
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20, "the call for effect was ignored");
    world.resource_mut::<QuestStageState>().set_stage(Q, 15);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 15, "the value call stopped the run");
}

/// `Game.TeachWord` / `Game.UnlockWord` give the player a word of power and raise `NVPE` once.
#[test]
fn teaching_a_word_records_it_and_raises_new_voice_power() {
    let script = parsed_source(
        "ScriptName QF_VmTest extends Quest\n\
         Form Property Marker Auto\n\
         Function Fragment_1()\n Game.TeachWord(Marker)\n Game.UnlockWord(Marker)\n EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let world = world_with_fragment(script);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<crate::words_of_power::KnownWords>().0.iter().copied().collect::<Vec<_>>(), [MARKER]);
    let raised = world
        .query::<crate::story_manager::StoryEvent>()
        .map(|events| events.iter().filter(|(_, e)| e.mnemonic == *b"NVPE").count())
        .unwrap_or(0);
    assert_eq!(raised, 1, "the second call finds the word known");
}

/// `Game.AdvanceSkill` queues the usage XP for the host's skill system.
#[test]
fn advance_skill_queues_the_usage_xp() {
    let script = parsed_source(
        "ScriptName QF_VmSkill extends Quest\n\
         Function Fragment_1()\n Game.AdvanceSkill(\"Lockpicking\", 50.0)\n EndFunction\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let world = world_with_fragment(script);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(
        world.resource::<crate::PendingSkillAdvances>().0,
        [("Lockpicking".to_owned(), 50.0)]
    );
}

/// BIK-REQ-002 / BIK-PRES-006 — in the VM, `Game.PlayBink` with a run-time
/// file name queues the movie and suspends the run until the movie ends (CK:
/// "It will not return until the movie is over").
#[test]
fn play_bink_freezes_the_run_until_the_movie_ends() {
    const SOURCE: &str = "ScriptName QF_VmBink extends Quest\n\
        Function Fragment_1()\n\
          String movie = \"BGS_Logo.bik\"\n\
          Game.PlayBink(movie, true)\n\
          Self.SetStage(20)\n\
        EndFunction\n";
    let (script, errors) = byroredux_papyrus::parse_script(SOURCE).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let script = Arc::new(script);
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        fragments.insert_vmad(Q, vmad());
        fragments.insert_vm_script(Arc::clone(&script));
        fragments.insert(
            Q,
            10,
            vec![Effect::Interpret { script: "QF_VmBink".into(), function: "Fragment_1".into(), args: Vec::new(), resume: None }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    let queued = world.resource::<crate::obscript_more::PendingVideos>().0.clone();
    assert_eq!(queued.len(), 1, "the movie is queued");
    assert_eq!((queued[0].file.as_str(), queued[0].interruptable, queued[0].mute_audio), ("BGS_Logo.bik", true, true));
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 10, "frozen behind the movie");
    world.resource_mut::<crate::obscript_more::PendingVideos>().0.clear();
    world.resource_mut::<crate::obscript_more::MoviePlaying>().0 = true;
    crate::fragment::fragment_continuation_system(&world, 1.0);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 10, "still playing");
    world.resource_mut::<crate::obscript_more::MoviePlaying>().0 = false;
    crate::fragment::fragment_continuation_system(&world, 0.0);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20, "resumed after the movie");
}

/// M48 GUI-EVT-001/002 — `RegisterForMenuOpenCloseEvent` registers the calling script for a menu
/// name; each registry opening and closing of that menu reaches its `OnMenuOpenCloseEvent(asMenuName,
/// abOpening)` as its own run, another menu's do not; the registration stands until it is
/// unregistered, and it ends when the owning quest stops.
#[test]
fn menu_open_close_events_reach_registered_scripts() {
    use byroredux_core::ecs::resources::menu_registry::{MenuGame, MenuRegistry, MenuSurface};
    let script = parsed_source(
        "ScriptName QF_VmMenuEvents extends Quest\n\
         Int Opened\n\
         Int Closed\n\
         Function Fragment_1()\n\
           RegisterForMenuOpenCloseEvent(\"ContainerMenu\")\n\
         EndFunction\n\
         Function Fragment_2()\n\
           UnregisterForMenuOpenCloseEvent(\"containermenu\")\n\
         EndFunction\n\
         Event OnMenuOpenCloseEvent(String asMenuName, Bool abOpening)\n\
           If asMenuName == \"ContainerMenu\"\n\
             If abOpening\n\
               Opened += 1\n\
             Else\n\
               Closed += 1\n\
             EndIf\n\
           EndIf\n\
         EndEvent\n",
    );
    assert_eq!(check_function_reason(Arc::clone(&script), "Fragment_1", None).err(), None);
    let mut world = world_with_fragment(script);
    world.insert_resource(crate::QuestScriptVariables::default());
    world.insert_resource(MenuRegistry::new(MenuGame::Fallout4));
    world.resource_mut::<crate::QuestStageFragments>().insert(
        Q,
        20,
        vec![Effect::Interpret { script: "QF_VmMenuEvents".into(), function: "Fragment_2".into(), args: Vec::new(), resume: None }],
    );
    let variable = |world: &World, name: &str| {
        world.resource::<crate::QuestScriptVariables>().get(Q, crate::QuestScriptVariables::property_key(name))
    };
    let menu = || MenuSurface::Movie { path: "interface\\containermenu.swf".into(), profile: "Fallout 4".into() };
    let toggle = |world: &World, name: &str, open: bool| {
        {
            let mut registry = world.resource_mut::<MenuRegistry>();
            if open {
                registry.open(name, menu());
            } else {
                registry.close(name);
            }
        }
        crate::menu_open_close_event_system(world, 0.0);
        crate::fragment::fragment_continuation_system(world, 0.0);
    };

    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<crate::MenuEventRegistrations>().entries().len(), 1);
    toggle(&world, "ContainerMenu", true);
    assert_eq!((variable(&world, "Opened"), variable(&world, "Closed")), (Some(1.0), None), "the opening");
    toggle(&world, "PauseMenu", true);
    toggle(&world, "PauseMenu", false);
    assert_eq!((variable(&world, "Opened"), variable(&world, "Closed")), (Some(1.0), None), "another menu is not heard");
    toggle(&world, "ContainerMenu", false);
    assert_eq!(variable(&world, "Closed"), Some(1.0), "the closing");
    toggle(&world, "ContainerMenu", true);
    assert_eq!(variable(&world, "Opened"), Some(2.0), "not one-shot");
    toggle(&world, "ContainerMenu", false);

    // Unregistered (the name compared case-insensitively): nothing more arrives.
    world.resource_mut::<QuestStageState>().set_stage(Q, 20);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert!(world.resource::<crate::MenuEventRegistrations>().entries().is_empty());
    toggle(&world, "ContainerMenu", true);
    toggle(&world, "ContainerMenu", false);
    assert_eq!((variable(&world, "Opened"), variable(&world, "Closed")), (Some(2.0), Some(2.0)));

    // GUI-EVT-002: registered again, then the quest stops — the registration goes with it.
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<crate::MenuEventRegistrations>().entries().len(), 1);
    world.resource_mut::<QuestStageState>().stop(Q);
    crate::reference_script_event_system(&world, 0.0);
    assert!(world.resource::<crate::MenuEventRegistrations>().entries().is_empty(), "the stopped quest unregistered");
    toggle(&world, "ContainerMenu", true);
    assert_eq!(variable(&world, "Opened"), Some(2.0));
}

/// A menu registration on another quest is not modelled: the check declines it (the run would
/// stop) instead of registering the wrong script.
#[test]
fn menu_open_close_registration_on_another_quest_is_declined() {
    let script = parsed_source(
        "ScriptName QF_VmMenuOther extends Quest\n\
         Quest Property Other Auto\n\
         Function Fragment_1()\n\
           Other.RegisterForMenuOpenCloseEvent(\"ContainerMenu\")\n\
         EndFunction\n",
    );
    let reason = check_function_reason(script, "Fragment_1", None).err().unwrap_or_default();
    assert!(reason.contains("registerformenuopencloseevent"), "{reason}");
}

/// M48 #6667 — `Message.Show()` for its value is latent (CK *Show - Message*: "returns the
/// button pressed"): MQ101's `If TempChooseSidesMessage.Show() == 0` raises the box, the run
/// waits for the answer, and the statement runs again with it, taking the branch it names. A
/// box a load took resumes with its default, button 0.
#[test]
fn message_show_waits_for_its_answer_and_branches_on_it() {
    const MESSAGE: u32 = 0x0001_0F10;
    const SOURCE: &str = "ScriptName QF_VmShow extends Quest\n\
        Message Property TempChooseSidesMessage Auto\n\
        Function Fragment_1()\n\
          Utility.Wait(1.0)\n\
          If TempChooseSidesMessage.Show() == 0\n\
            Self.SetStage(6)\n\
          Else\n\
            Self.SetStage(7)\n\
          EndIf\n\
        EndFunction\n";
    // The same branch through a box wrapper called for its value: the calling statement waits on the
    // wrapper's box and replays with the answer.
    const WRAPPED: &str = "ScriptName QF_VmShow extends Quest\n\
        Message Property TempChooseSidesMessage Auto\n\
        Int Function Ask()\n\
          Return TempChooseSidesMessage.Show()\n\
        EndFunction\n\
        Function Fragment_1()\n\
          Utility.Wait(1.0)\n\
          If Ask() == 0\n\
            Self.SetStage(6)\n\
          Else\n\
            Self.SetStage(7)\n\
          EndIf\n\
        EndFunction\n";
    let run = |source: &str, answer: Option<usize>| {
        let (script, errors) = byroredux_papyrus::parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let script = Arc::new(script);
        if let Err(why) = crate::fragment::vm::checker::check_function_reason(Arc::clone(&script), "Fragment_1", None) { panic!("{why}") }
        let mut world = World::new();
        crate::register(&mut world);
        let player = world.spawn();
        world.insert_resource(PapyrusPlayerEntity(player));
        world.insert_resource(QuestStageState::default());
        let mut index = byroredux_plugin::esm::records::EsmIndex::default();
        index.messages.insert(
            MESSAGE,
            byroredux_plugin::esm::records::MesgRecord {
                form_id: MESSAGE,
                description: "Who will you follow?".into(),
                message_box: true,
                buttons: vec!["Hadvar".into(), "Ralof".into()],
                ..Default::default()
            },
        );
        crate::install_message_catalog(&mut world, &index);
        let mut vmad = vmad();
        vmad.scripts[0].name = "QF_VmShow".into();
        vmad.scripts[0].properties.push(ScriptProperty {
            name: "TempChooseSidesMessage".into(),
            status: 1,
            value: PropertyValue::Object { form_id: MESSAGE, alias: -1 },
        });
        {
            let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
            fragments.insert_vmad(Q, vmad);
            fragments.insert_vm_script(Arc::clone(&script));
            fragments.insert(
                Q,
                0,
                vec![Effect::Interpret { script: "QF_VmShow".into(), function: "Fragment_1".into(), args: Vec::new(), resume: None }],
            );
        }
        world.resource_mut::<QuestStageState>().set_stage(Q, 0);
        crate::fragment::quest_fragment_dispatch_system(&world);
        crate::fragment::fragment_continuation_system(&world, 1.0);
        let raised = std::mem::take(&mut world.resource_mut::<crate::PendingScriptMessages>().0);
        assert_eq!(raised.len(), 1, "the box is up");
        let crate::MessageAnswer::Papyrus(ticket) = raised[0].answer else { panic!("{:?}", raised[0].answer) };
        crate::fragment::fragment_continuation_system(&world, 5.0);
        assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 0, "the run waits for the answer");
        match answer {
            Some(button) => crate::messages::record_papyrus_answer(&world, ticket, button),
            // A load drops the box and its ticket.
            None => world.resource_mut::<crate::messages::PapyrusMessageAnswers>().forget_boxes(),
        }
        crate::fragment::fragment_continuation_system(&world, 0.0);
        assert!(world.resource::<crate::PendingScriptMessages>().0.is_empty(), "the replay reads the answer; no second box");
        world.resource::<QuestStageState>().get_stage(Q)
    };
    for source in [SOURCE, WRAPPED] {
        assert_eq!(run(source, Some(0)), 6, "Hadvar");
        assert_eq!(run(source, Some(1)), 7, "Ralof");
        assert_eq!(run(source, None), 6, "a box a load took resumes with its default");
    }
}

/// The native wave (#5800 families): a door's lock and open state and its cell, its one script's state, an
/// actor's commanded flag, shield and right-hand weapon, and a package's template, as the CK pages give them.
#[test]
fn the_native_wave_reads_locks_doors_cells_states_shields_weapons_and_templates() {
    use byroredux_core::ecs::components::{CellFormId, EquipmentSlots, EquippedWeapon, Inventory, InventoryIndex, ItemStack, Locked};
    const DOOR: u32 = 0x0000_D002;
    const CELL: u32 = 0x0000_CE11;
    const SHIELD: u32 = 0x0000_5E1D;
    const SWORD: u32 = 0x0000_5A0D;
    const PACKAGE: u32 = 0x0000_9A01;
    const TEMPLATE: u32 = 0x0000_9A02;
    let script = parsed_source(
        "ScriptName QF_VmNatives extends Quest\n\
         ObjectReference Property Door Auto\n\
         Package Property Pkg Auto\n\
         Armor Property Shield Auto\n\
         Weapon Property Sword Auto\n\
         Package Property Template Auto\n\
         Int Property Results Auto\n\
         Int Property Open Auto\n\
         Function Fragment_1()\n\
           Actor player = Game.GetPlayer()\n\
           If Door.IsLocked()\n Results += 1\n EndIf\n\
           Open = Door.GetOpenState()\n\
           If Door.GetParentCell() == player.GetParentCell()\n Results += 10\n EndIf\n\
           If !player.IsCommandedActor()\n Results += 100\n EndIf\n\
           If player.GetEquippedShield() == Shield\n Results += 1000\n EndIf\n\
           If player.GetEquippedWeapon() == Sword\n Results += 10000\n EndIf\n\
           If Pkg.GetTemplate() == Template\n Results += 100000\n EndIf\n\
           If Door.GetState() == \"Waiting\"\n Results += 1000000\n EndIf\n\
         EndFunction\n",
    );
    let door_script = parsed_source("ScriptName VmDoorScript extends ObjectReference\nAuto State Waiting\nEndState\n");
    let mut world = World::new();
    crate::register(&mut world);
    world.insert_resource(crate::ScriptGame(byroredux_plugin::esm::reader::GameKind::Skyrim));
    world.insert_resource(QuestStageState::default());
    world.insert_resource(crate::QuestScriptVariables::default());
    world.insert_resource(crate::QuestScriptObjects::default());
    world.insert_resource(crate::package::PackageRegistry::from_records([
        byroredux_plugin::esm::records::PackRecord { form_id: PACKAGE, package_template_form_id: Some(TEMPLATE), ..Default::default() },
        byroredux_plugin::esm::records::PackRecord { form_id: TEMPLATE, ..Default::default() },
    ]));
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let door_id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(DOOR) });
    world.insert_resource(pool);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert(player, CellFormId(CELL));
    let mut inventory = Inventory::new();
    inventory.push(ItemStack::new(SHIELD, 1));
    inventory.push(ItemStack::new(SWORD, 1));
    world.insert(player, inventory);
    let mut slots = EquipmentSlots::new();
    slots.equip(1 << 9, InventoryIndex(0));
    slots.equip_weapon(InventoryIndex(1));
    world.insert(player, slots);
    world.insert(player, EquippedWeapon { inventory_index: InventoryIndex(1), base_form_id: SWORD, damage: 7.0, reach: 1.0, speed: 1.0 });
    let door = world.spawn();
    world.insert(door, FormIdComponent(door_id));
    world.insert(door, CellFormId(CELL));
    world.insert(door, Locked { lock_level: 25, key_form_id: None });
    world.insert(door, crate::vm_state::TwoStateActivator { is_open: true, ..Default::default() });
    world.insert(
        door,
        crate::fragment::PapyrusReferenceScripts(vec![crate::fragment::ReferenceScript {
            script: "VmDoorScript".into(),
            vmad: Arc::new(ScriptInstanceData { version: 5, object_format: 2, scripts: Vec::new() }),
            events: Vec::new(),
        }]),
    );
    let object = |name: &str, form_id: u32| ScriptProperty { name: name.into(), status: 1, value: PropertyValue::Object { form_id, alias: -1 } };
    let vmad = ScriptInstanceData {
        version: 5,
        object_format: 2,
        scripts: vec![ScriptInstance {
            name: "QF_VmNatives".into(),
            status: 0,
            properties: vec![
                object("Door", DOOR),
                object("Pkg", PACKAGE),
                object("Shield", SHIELD),
                object("Sword", SWORD),
                object("Template", TEMPLATE),
            ],
        }],
    };
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        fragments.insert_vmad(Q, vmad);
        fragments.insert_vm_script(Arc::clone(&script));
        fragments.insert_vm_script(door_script);
        fragments.insert(
            Q,
            10,
            vec![Effect::Interpret { script: "QF_VmNatives".into(), function: "Fragment_1".into(), args: Vec::new(), resume: None }],
        );
    }
    if let Err(why) = crate::fragment::vm::checker::check_function_reason(Arc::clone(&script), "Fragment_1", None) {
        panic!("{why}")
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(quest_property(&world, "Results"), Some(1_111_111.0), "every read answered as the CK gives it");
    assert_eq!(quest_property(&world, "Open"), Some(1.0), "an open, settled door is 1 (Open)");
}

/// Hand Equipment #7398: `GetEquippedItemType` reads each hand (0 left, 1 right) by the CK's codes and
/// `GetEquippedWeapon(true)` the left hand's weapon; an item type other than 0 or 1 is declined by the
/// checker.
#[test]
fn equipped_item_type_and_left_weapon_read_each_hand() {
    use crate::hands::{HandCatalog, HandWeapon};
    use byroredux_core::ecs::components::{EquipmentSlots, Inventory, InventoryIndex, ItemStack};
    use byroredux_plugin::esm::records::SlotHands;
    const SWORD: u32 = 0x0000_5A0D;
    const DAGGER: u32 = 0x0000_5A0E;
    let script = parsed_source(
        "ScriptName QF_VmHands extends Quest\n\
         Weapon Property Dagger Auto\n\
         Weapon Property Sword Auto\n\
         Int Property Left Auto\n\
         Int Property Right Auto\n\
         Int Property Results Auto\n\
         Function Fragment_1()\n\
           Actor player = Game.GetPlayer()\n\
           Left = player.GetEquippedItemType(0)\n\
           Right = player.GetEquippedItemType(1)\n\
           If player.GetEquippedWeapon(true) == Dagger\n Results += 1\n EndIf\n\
           If player.GetEquippedWeapon() == Sword\n Results += 10\n EndIf\n\
         EndFunction\n\
         Function Fragment_2()\n\
           Left = Game.GetPlayer().GetEquippedItemType(2)\n\
         EndFunction\n",
    );
    let mut world = World::new();
    crate::register(&mut world);
    world.insert_resource(crate::ScriptGame(byroredux_plugin::esm::reader::GameKind::Skyrim));
    world.insert_resource(QuestStageState::default());
    world.insert_resource(crate::QuestScriptVariables::default());
    world.insert_resource(crate::QuestScriptObjects::default());
    let mut hands = HandCatalog::default();
    hands.insert_weapon(SWORD, HandWeapon { anim_type: 1, hands: SlotHands::Right });
    hands.insert_weapon(DAGGER, HandWeapon { anim_type: 2, hands: SlotHands::Right });
    world.insert_resource(hands);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    let mut inventory = Inventory::new();
    inventory.push(ItemStack::new(SWORD, 1));
    inventory.push(ItemStack::new(DAGGER, 1));
    world.insert(player, inventory);
    let mut slots = EquipmentSlots::new();
    slots.equip_weapon(InventoryIndex(0));
    slots.equip_left_hand(InventoryIndex(1));
    world.insert(player, slots);
    let object = |name: &str, form_id: u32| ScriptProperty { name: name.into(), status: 1, value: PropertyValue::Object { form_id, alias: -1 } };
    let vmad = ScriptInstanceData {
        version: 5,
        object_format: 2,
        scripts: vec![ScriptInstance {
            name: "QF_VmHands".into(),
            status: 0,
            properties: vec![object("Dagger", DAGGER), object("Sword", SWORD)],
        }],
    };
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        fragments.insert_vmad(Q, vmad);
        fragments.insert_vm_script(Arc::clone(&script));
        fragments.insert(
            Q,
            10,
            vec![Effect::Interpret { script: "QF_VmHands".into(), function: "Fragment_1".into(), args: Vec::new(), resume: None }],
        );
    }
    if let Err(why) = crate::fragment::vm::checker::check_function_reason(Arc::clone(&script), "Fragment_1", None) {
        panic!("{why}")
    }
    assert!(
        crate::fragment::vm::checker::check_function_reason(Arc::clone(&script), "Fragment_2", None).is_err(),
        "a hand other than 0 or 1 is declined"
    );
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(quest_property(&world, "Left"), Some(2.0), "a dagger in the left hand");
    assert_eq!(quest_property(&world, "Right"), Some(1.0), "a sword in the right");
    assert_eq!(quest_property(&world, "Results"), Some(11.0), "each hand's weapon");
}

/// Hand Equipment #7399: `EquipSpell` readies a spell in the hand it names (CK source 0 left, 1 right)
/// and `GetEquippedSpell` / `GetEquippedItemType` (9) read it back; `UnequipSpell` takes it out;
/// `EquipItemEx(item, 2)` queues the item for the left hand; `IsWeaponDrawn` reads the drawn state.
#[test]
fn equip_spell_and_equip_item_ex_reach_the_hands() {
    use crate::hands::{Hand, HandCatalog, HandWeapon};
    use byroredux_core::ecs::components::{EquipmentSlots, Inventory, InventoryIndex, ItemStack};
    use byroredux_plugin::esm::records::SlotHands;
    const SWORD: u32 = 0x0000_5A0D;
    const DAGGER: u32 = 0x0000_5A0E;
    const FLAMES: u32 = 0x0000_5A0F;
    let script = parsed_source(
        "ScriptName QF_VmSpells extends Quest\n\
         Weapon Property Dagger Auto\n\
         Spell Property Flames Auto\n\
         Int Property Left Auto\n\
         Int Property Right Auto\n\
         Int Property Results Auto\n\
         Function Fragment_1()\n\
           Actor player = Game.GetPlayer()\n\
           player.EquipSpell(Flames, 0)\n\
           Left = player.GetEquippedItemType(0)\n\
           If player.GetEquippedSpell(0) == Flames\n Results += 1\n EndIf\n\
           If !player.IsWeaponDrawn()\n Results += 10\n EndIf\n\
           player.UnequipSpell(Flames, 0)\n\
           Right = player.GetEquippedItemType(0)\n\
           player.EquipItemEx(Dagger, 2)\n\
         EndFunction\n",
    );
    let mut world = World::new();
    crate::register(&mut world);
    world.insert_resource(crate::ScriptGame(byroredux_plugin::esm::reader::GameKind::Skyrim));
    world.insert_resource(QuestStageState::default());
    world.insert_resource(crate::QuestScriptVariables::default());
    world.insert_resource(crate::QuestScriptObjects::default());
    let mut hands = HandCatalog::default();
    hands.insert_weapon(SWORD, HandWeapon { anim_type: 1, hands: SlotHands::Right });
    hands.insert_weapon(DAGGER, HandWeapon { anim_type: 2, hands: SlotHands::Right });
    hands.insert_spell(FLAMES, SlotHands::Either);
    world.insert_resource(hands);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    let mut inventory = Inventory::new();
    inventory.push(ItemStack::new(SWORD, 1));
    inventory.push(ItemStack::new(DAGGER, 1));
    world.insert(player, inventory);
    let mut slots = EquipmentSlots::new();
    slots.equip_weapon(InventoryIndex(0));
    world.insert(player, slots);
    let object = |name: &str, form_id: u32| ScriptProperty { name: name.into(), status: 1, value: PropertyValue::Object { form_id, alias: -1 } };
    let vmad = ScriptInstanceData {
        version: 5,
        object_format: 2,
        scripts: vec![ScriptInstance {
            name: "QF_VmSpells".into(),
            status: 0,
            properties: vec![object("Dagger", DAGGER), object("Flames", FLAMES)],
        }],
    };
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        fragments.insert_vmad(Q, vmad);
        fragments.insert_vm_script(Arc::clone(&script));
        fragments.insert(
            Q,
            10,
            vec![Effect::Interpret { script: "QF_VmSpells".into(), function: "Fragment_1".into(), args: Vec::new(), resume: None }],
        );
    }
    if let Err(why) = crate::fragment::vm::checker::check_function_reason(Arc::clone(&script), "Fragment_1", None) {
        panic!("{why}")
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(quest_property(&world, "Left"), Some(9.0), "a spell in the left hand");
    assert_eq!(quest_property(&world, "Results"), Some(11.0), "the left hand's spell, and no weapon drawn");
    assert_eq!(quest_property(&world, "Right"), Some(0.0), "UnequipSpell empties the hand");
    assert_eq!(world.get::<EquipmentSlots>(player).and_then(|slots| slots.weapon), Some(InventoryIndex(0)), "the right hand kept its sword");
    assert_eq!(
        world.resource::<crate::equipment::PendingNonArmorEquips>().0,
        vec![(player, DAGGER, Hand::Left)],
        "EquipItemEx(item, 2) asks for the left hand"
    );
}

/// LMC Q5 / LMC-ID-005: a mod script reads a shipped `JsonUtil` file, writes values, lists and a form in
/// PapyrusUtil's format, reaches `StorageUtil`'s file functions (`../StorageUtil.json`), and reads SKSE's
/// load-order functions; the changes are written when the game saves.
#[test]
fn json_util_storage_util_files_and_mod_index_reads() {
    use crate::json_util::{tests::MemoryJsonFiles, JsonUtilStore};
    use byroredux_core::form_id::PluginId;
    use byroredux_plugin::esm::reader::GlobalSlot;
    use byroredux_sdk::content::{ContentCatalog, PluginInfo, PluginKind};
    const SWORD: u32 = 0x0001_2EB7;
    let script = parsed_source(
        "ScriptName QF_VmJson extends Quest\n\
         Weapon Property Sword Auto\n\
         Int Property Results Auto\n\
         Int Property Difficulty Auto\n\
         Int Property Duplicate Auto\n\
         Function Fragment_1()\n\
           Difficulty = JsonUtil.GetIntValue(\"MyMod/Settings\", \"Difficulty\", 1)\n\
           JsonUtil.SetIntValue(\"MyMod/Settings\", \"Visits\", 2)\n\
           JsonUtil.IntListAdd(\"MyMod/Settings\", \"Levels\", 3)\n\
           Duplicate = JsonUtil.IntListAdd(\"MyMod/Settings\", \"Levels\", 3, false)\n\
           If JsonUtil.IntListCount(\"MyMod/Settings\", \"levels\") == 1\n Results += 1\n EndIf\n\
           JsonUtil.SetFormValue(\"MyMod/Settings\", \"Item\", Sword)\n\
           If JsonUtil.GetFormValue(\"MyMod/Settings\", \"item\") == Sword\n Results += 10\n EndIf\n\
           StorageUtil.FileSetStringValue(\"Name\", \"Lydia\")\n\
           If StorageUtil.FileGetStringValue(\"name\") == \"Lydia\"\n Results += 100\n EndIf\n\
           If Game.GetModByName(\"Skyrim.esm\") == 0 && Game.GetModByName(\"Missing.esp\") == 255\n Results += 1000\n EndIf\n\
           If Game.IsPluginInstalled(\"Update.esm\") && !Game.IsPluginInstalled(\"Missing.esp\")\n Results += 10000\n EndIf\n\
           If Game.GetModName(1) == \"Update.esm\" && Game.GetModCount() == 2\n Results += 100000\n EndIf\n\
         EndFunction\n",
    );
    let mut world = World::new();
    crate::register(&mut world);
    world.insert_resource(crate::ScriptGame(byroredux_plugin::esm::reader::GameKind::Skyrim));
    world.insert_resource(QuestStageState::default());
    world.insert_resource(crate::QuestScriptVariables::default());
    world.insert_resource(crate::QuestScriptObjects::default());
    let plugin = |name: &str| PluginInfo::new(name, PluginId::from_filename(name).0.to_be_bytes(), PluginKind::Regular).unwrap();
    crate::set_legacy_obscript_content_catalog(
        &world,
        std::sync::Arc::new(ContentCatalog::new(vec![plugin("Skyrim.esm"), plugin("Update.esm")]).unwrap()),
    );
    world.insert_resource(crate::LoadOrderIdentity::new(vec![
        (GlobalSlot::Regular(0), PluginId::from_filename("Skyrim.esm")),
        (GlobalSlot::Regular(1), PluginId::from_filename("Update.esm")),
    ]));
    let files = std::sync::Arc::new(MemoryJsonFiles {
        shipped: std::collections::HashMap::from([(
            "skse/plugins/storageutildata/mymod/settings.json".to_owned(),
            r#"{ "int": { "difficulty": 4 } }"#.to_owned(),
        )]),
        ..Default::default()
    });
    world.insert_resource(JsonUtilStore::new(files.clone()));
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    let vmad = ScriptInstanceData {
        version: 5,
        object_format: 2,
        scripts: vec![ScriptInstance {
            name: "QF_VmJson".into(),
            status: 0,
            properties: vec![ScriptProperty { name: "Sword".into(), status: 1, value: PropertyValue::Object { form_id: SWORD, alias: -1 } }],
        }],
    };
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        fragments.insert_vmad(Q, vmad);
        fragments.insert_vm_script(Arc::clone(&script));
        fragments.insert(
            Q,
            10,
            vec![Effect::Interpret { script: "QF_VmJson".into(), function: "Fragment_1".into(), args: Vec::new(), resume: None }],
        );
    }
    if let Err(why) = crate::fragment::vm::checker::check_function_reason(Arc::clone(&script), "Fragment_1", None) {
        panic!("{why}")
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(quest_property(&world, "Difficulty"), Some(4.0), "the shipped file's value, its key in any case");
    assert_eq!(quest_property(&world, "Duplicate"), Some(-1.0), "no duplicate when not allowed");
    assert_eq!(quest_property(&world, "Results"), Some(111_111.0), "every read answered");
    assert!(files.profile.lock().unwrap().is_empty(), "nothing written before the game saves");

    assert_eq!(world.resource_mut::<JsonUtilStore>().save_modified(), 2);
    let profile = files.profile.lock().unwrap();
    let settings: serde_json::Value = serde_json::from_str(&profile["skse/plugins/storageutildata/mymod/settings.json"]).unwrap();
    assert_eq!(settings["int"], serde_json::json!({ "difficulty": 4, "visits": 2 }));
    assert_eq!(settings["intList"]["levels"], serde_json::json!([3]));
    assert_eq!(settings["form"]["item"], serde_json::json!("0x12eb7|Skyrim.esm"), "PapyrusUtil's form string");
    let storage: serde_json::Value = serde_json::from_str(&profile["skse/plugins/storageutil.json"]).unwrap();
    assert_eq!(storage["string"]["name"], serde_json::json!("Lydia"));
}

/// LMC Q5: `StorageUtil`'s values on a form and globally (set, pluck, a zero unsets, list shift / pop / sort,
/// prefix counts across objects, the debug listings) and `JsonUtil`'s paths and `ListRandom`.
#[test]
fn storage_util_form_values_and_json_util_paths() {
    use crate::json_util::{tests::MemoryJsonFiles, JsonUtilStore, StorageUtilValues};
    use byroredux_core::form_id::PluginId;
    use byroredux_plugin::esm::reader::GlobalSlot;
    use byroredux_sdk::content::{ContentCatalog, PluginInfo, PluginKind};
    const LYDIA: u32 = 0x000A_2C94;
    let script = parsed_source(
        "ScriptName QF_VmStorage extends Quest\n\
         Actor Property Lydia Auto\n\
         Int Property Results Auto\n\
         Int Property Plucked Auto\n\
         Int Property Shifted Auto\n\
         Int Property Prefixed Auto\n\
         Int Property PathValue Auto\n\
         Function Fragment_1()\n\
           StorageUtil.SetIntValue(Lydia, \"mymod_mood\", 3)\n\
           StorageUtil.SetIntValue(None, \"mymod_visits\", 7)\n\
           If StorageUtil.GetIntValue(Lydia, \"MyMod_Mood\") == 3 && StorageUtil.GetIntValue(None, \"mymod_visits\") == 7\n Results += 1\n EndIf\n\
           Plucked = StorageUtil.PluckIntValue(None, \"mymod_visits\")\n\
           If !StorageUtil.HasIntValue(None, \"mymod_visits\")\n Results += 10\n EndIf\n\
           StorageUtil.SetIntValue(Lydia, \"temp\", 5)\n\
           StorageUtil.SetIntValue(Lydia, \"temp\", 0)\n\
           If !StorageUtil.HasIntValue(Lydia, \"temp\")\n Results += 100\n EndIf\n\
           StorageUtil.IntListAdd(Lydia, \"mymod_order\", 3)\n\
           StorageUtil.IntListAdd(Lydia, \"mymod_order\", 1)\n\
           StorageUtil.IntListAdd(Lydia, \"mymod_order\", 2)\n\
           StorageUtil.IntListSort(Lydia, \"mymod_order\")\n\
           Shifted = StorageUtil.IntListShift(Lydia, \"mymod_order\")\n\
           If StorageUtil.IntListPop(Lydia, \"mymod_order\") == 3 && StorageUtil.IntListCount(Lydia, \"mymod_order\") == 1\n Results += 1000\n EndIf\n\
           Prefixed = StorageUtil.CountAllPrefix(\"mymod_\")\n\
           If StorageUtil.debug_GetIntObject(0) == Lydia && StorageUtil.debug_GetIntObjectCount() == 1\n Results += 10000\n EndIf\n\
           JsonUtil.SetPathIntValue(\"paths\", \".foo.bar[1]\", 10)\n\
           PathValue = JsonUtil.GetPathIntValue(\"paths\", \"foo.bar[1]\")\n\
           If JsonUtil.PathCount(\"paths\", \".foo.bar\") == 2 && JsonUtil.IsPathArray(\"paths\", \".foo.bar\") && !JsonUtil.CanResolvePath(\"paths\", \".nope\")\n Results += 100000\n EndIf\n\
           JsonUtil.IntListAdd(\"paths\", \"one\", 42)\n\
           If JsonUtil.IntListRandom(\"paths\", \"one\") == 42\n Results += 1000000\n EndIf\n\
         EndFunction\n",
    );
    let mut world = World::new();
    crate::register(&mut world);
    world.insert_resource(crate::ScriptGame(byroredux_plugin::esm::reader::GameKind::Skyrim));
    world.insert_resource(QuestStageState::default());
    world.insert_resource(crate::QuestScriptVariables::default());
    world.insert_resource(crate::QuestScriptObjects::default());
    let plugin = |name: &str| PluginInfo::new(name, PluginId::from_filename(name).0.to_be_bytes(), PluginKind::Regular).unwrap();
    crate::set_legacy_obscript_content_catalog(&world, std::sync::Arc::new(ContentCatalog::new(vec![plugin("Skyrim.esm")]).unwrap()));
    world.insert_resource(crate::LoadOrderIdentity::new(vec![(GlobalSlot::Regular(0), PluginId::from_filename("Skyrim.esm"))]));
    world.insert_resource(JsonUtilStore::new(std::sync::Arc::new(MemoryJsonFiles::default())));
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    let vmad = ScriptInstanceData {
        version: 5,
        object_format: 2,
        scripts: vec![ScriptInstance {
            name: "QF_VmStorage".into(),
            status: 0,
            properties: vec![ScriptProperty { name: "Lydia".into(), status: 1, value: PropertyValue::Object { form_id: LYDIA, alias: -1 } }],
        }],
    };
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        fragments.insert_vmad(Q, vmad);
        fragments.insert_vm_script(Arc::clone(&script));
        fragments.insert(
            Q,
            10,
            vec![Effect::Interpret { script: "QF_VmStorage".into(), function: "Fragment_1".into(), args: Vec::new(), resume: None }],
        );
    }
    if let Err(why) = crate::fragment::vm::checker::check_function_reason(Arc::clone(&script), "Fragment_1", None) {
        panic!("{why}")
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(quest_property(&world, "Plucked"), Some(7.0), "a pluck returns the value");
    assert_eq!(quest_property(&world, "Shifted"), Some(1.0), "sorted, then shifted from the front");
    assert_eq!(quest_property(&world, "Prefixed"), Some(2.0), "mymod_mood and mymod_order on Lydia");
    assert_eq!(quest_property(&world, "PathValue"), Some(10.0));
    assert_eq!(quest_property(&world, "Results"), Some(1_111_111.0), "every read answered");
    let mut values = world.resource_mut::<StorageUtilValues>();
    let lydia = values.doc("0xa2c94|Skyrim.esm").get(crate::json_util::Kind::Int, "mymod_mood");
    assert_eq!(lydia, Some(serde_json::json!(3)), "keyed by the form's plugin and id");
}

/// LMC Q5: a mod script's JContainers calls through a fragment: a `JMap` with an array and a form map in it, `JDB`
/// and `JFormDB` paths, `JAtomic`, a file written and read back in JContainers' format, and the version checks.
#[test]
fn jcontainers_objects_paths_files_and_version() {
    use crate::jcontainers::JcHeap;
    use crate::json_util::{tests::MemoryJsonFiles, JsonUtilStore};
    use byroredux_core::form_id::PluginId;
    use byroredux_plugin::esm::reader::GlobalSlot;
    use byroredux_sdk::content::{ContentCatalog, PluginInfo, PluginKind};
    const LYDIA: u32 = 0x000A_2C94;
    let script = parsed_source(
        "ScriptName QF_VmJc extends Quest\n\
         Actor Property Lydia Auto\n\
         Int Property Results Auto\n\
         Int Property Previous Auto\n\
         Int Property Copy Auto\n\
         Function Fragment_1()\n\
           int jSettings = JMap.object()\n\
           JMap.setInt(jSettings, \"Difficulty\", 3)\n\
           int jLevels = JArray.objectWithInts(new int[3])\n\
           JArray.setInt(jLevels, -1, 7)\n\
           JMap.setObj(jSettings, \"levels\", jLevels)\n\
           int jByActor = JFormMap.object()\n\
           JFormMap.setStr(jByActor, Lydia, \"housecarl\")\n\
           JMap.setObj(jSettings, \"byActor\", jByActor)\n\
           JDB.setObj(\"mymod\", jSettings)\n\
           If JDB.solveInt(\".mymod.difficulty\") == 3 && JDB.solveInt(\".mymod.levels[2]\") == 7\n Results += 1\n EndIf\n\
           If JValue.solveStr(jSettings, \".byActor[__formData|Skyrim.esm|0xa2c94]\") == \"housecarl\"\n Results += 10\n EndIf\n\
           JFormDB.solveIntSetter(Lydia, \".mymodforms.mood\", 5, true)\n\
           If JFormDB.getInt(Lydia, \".mymodforms.mood\") == 5 && JFormDB.findEntry(\"mymodforms\", Lydia) != 0\n Results += 100\n EndIf\n\
           Previous = JAtomic.fetchAddInt(jSettings, \".difficulty\", 2)\n\
           If JMap.getInt(jSettings, \"DIFFICULTY\") == 5\n Results += 1000\n EndIf\n\
           JValue.writeToFile(jSettings, \"Data/SKSE/Plugins/MyMod/settings.json\")\n\
           Copy = JValue.readFromFile(\"Data/SKSE/Plugins/MyMod/settings.json\")\n\
           If JValue.solveInt(Copy, \".levels[2]\") == 7 && JValue.solveForm(Copy, \".byActor[__formData|Skyrim.esm|0xa2c94]\") == None\n Results += 10000\n EndIf\n\
           If JContainers.isInstalled() && JContainers.versionAtLeast(4, 3) && JContainers.APIVersion() == 4\n Results += 100000\n EndIf\n\
         EndFunction\n",
    );
    let mut world = World::new();
    crate::register(&mut world);
    world.insert_resource(crate::ScriptGame(byroredux_plugin::esm::reader::GameKind::Skyrim));
    world.insert_resource(QuestStageState::default());
    world.insert_resource(crate::QuestScriptVariables::default());
    world.insert_resource(crate::QuestScriptObjects::default());
    let plugin = |name: &str| PluginInfo::new(name, PluginId::from_filename(name).0.to_be_bytes(), PluginKind::Regular).unwrap();
    crate::set_legacy_obscript_content_catalog(&world, std::sync::Arc::new(ContentCatalog::new(vec![plugin("Skyrim.esm")]).unwrap()));
    world.insert_resource(crate::LoadOrderIdentity::new(vec![(GlobalSlot::Regular(0), PluginId::from_filename("Skyrim.esm"))]));
    let files = std::sync::Arc::new(MemoryJsonFiles::default());
    world.insert_resource(JsonUtilStore::new(files.clone()));
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    let vmad = ScriptInstanceData {
        version: 5,
        object_format: 2,
        scripts: vec![ScriptInstance {
            name: "QF_VmJc".into(),
            status: 0,
            properties: vec![ScriptProperty { name: "Lydia".into(), status: 1, value: PropertyValue::Object { form_id: LYDIA, alias: -1 } }],
        }],
    };
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        fragments.insert_vmad(Q, vmad);
        fragments.insert_vm_script(Arc::clone(&script));
        fragments.insert(
            Q,
            10,
            vec![Effect::Interpret { script: "QF_VmJc".into(), function: "Fragment_1".into(), args: Vec::new(), resume: None }],
        );
    }
    if let Err(why) = crate::fragment::vm::checker::check_function_reason(Arc::clone(&script), "Fragment_1", None) {
        panic!("{why}")
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(quest_property(&world, "Previous"), Some(3.0), "fetchAdd returns the value before");
    assert_eq!(quest_property(&world, "Results"), Some(111_111.0), "every read answered");
    let written: serde_json::Value = serde_json::from_str(&files.profile.lock().unwrap()["skse/plugins/mymod/settings.json"]).unwrap();
    assert_eq!(written["levels"], serde_json::json!([0, 0, 7]));
    assert_eq!(written["byActor"]["__metaInfo"]["typeName"], serde_json::json!("JFormMap"));
    assert_eq!(written["byActor"]["__formData|Skyrim.esm|0xa2c94"], serde_json::json!("housecarl"));
    // JDB holds the settings: a sweep long after keeps them, and drops nothing JDB reaches.
    let mut heap = world.resource_mut::<JcHeap>();
    let alive = heap.len();
    heap.sweep(1.0e9);
    assert!(heap.len() >= 4 && heap.len() < alive, "JDB's tree stays; the unowned copy read from file goes");
}

/// The second native wave: `SetScale` scales a reference, `SetRestrained` restrains an actor other than
/// the player, and `MoveToNode` puts a reference on the named node of another's 3D (its position and
/// rotation), as the CK pages give them.
#[test]
fn set_scale_set_restrained_and_move_to_node_act_on_their_references() {
    use byroredux_core::ecs::components::{Children, GlobalTransform, Name, Transform};
    use byroredux_core::math::{Quat, Vec3};
    const ITEM: u32 = 0x0000_1701;
    const RACK: u32 = 0x0000_1702;
    const GUARD: u32 = 0x0000_1703;
    let script = parsed_source(
        "ScriptName QF_VmPlace extends Quest\n\
         ObjectReference Property Item Auto\n\
         ObjectReference Property Rack Auto\n\
         Actor Property Guard Auto\n\
         Function Fragment_1()\n\
           Item.SetScale(2.5)\n\
           Guard.SetRestrained()\n\
           Item.MoveToNode(Rack, \"SwordNode\")\n\
         EndFunction\n",
    );
    if let Err(why) = crate::fragment::vm::checker::check_function_reason(Arc::clone(&script), "Fragment_1", None) {
        panic!("{why}")
    }
    let mut world = World::new();
    crate::register(&mut world);
    world.insert_resource(QuestStageState::default());
    world.register::<FormIdComponent>();
    world.register::<crate::ActorControlState>();
    let mut pool = FormIdPool::default();
    let mut id = |local: u32| pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(local) });
    let (item_id, rack_id, guard_id) = (id(ITEM), id(RACK), id(GUARD));
    world.insert_resource(pool);
    let mut strings = byroredux_core::string::StringPool::default();
    let node_name = strings.intern("SwordNode");
    world.insert_resource(strings);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    let item = world.spawn();
    world.insert(item, FormIdComponent(item_id));
    world.insert(item, Transform::default());
    let rack = world.spawn();
    world.insert(rack, FormIdComponent(rack_id));
    world.insert(rack, Transform::default());
    let node = world.spawn();
    world.insert(node, Name(node_name));
    let at = Vec3::new(10.0, 20.0, 30.0);
    let turned = Quat::from_rotation_y(1.0);
    world.insert(node, GlobalTransform::new(at, turned, 1.0));
    world.insert(rack, Children(vec![node]));
    let guard = world.spawn();
    world.insert(guard, FormIdComponent(guard_id));
    let object = |name: &str, form_id: u32| ScriptProperty { name: name.into(), status: 1, value: PropertyValue::Object { form_id, alias: -1 } };
    let vmad = ScriptInstanceData {
        version: 5,
        object_format: 2,
        scripts: vec![ScriptInstance {
            name: "QF_VmPlace".into(),
            status: 0,
            properties: vec![object("Item", ITEM), object("Rack", RACK), object("Guard", GUARD)],
        }],
    };
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        fragments.insert_vmad(Q, vmad);
        fragments.insert_vm_script(script);
        fragments.insert(
            Q,
            10,
            vec![Effect::Interpret { script: "QF_VmPlace".into(), function: "Fragment_1".into(), args: Vec::new(), resume: None }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    let placed = *world.get::<Transform>(item).unwrap();
    assert_eq!(placed.scale, 2.5, "SetScale");
    assert!((placed.translation - at).length() < 1e-4, "MoveToNode takes the node's position");
    assert!(placed.rotation.abs_diff_eq(turned, 1e-5), "and its rotation");
    assert!(world.get::<crate::ActorControlState>(guard).is_some_and(|state| state.restrained), "SetRestrained");
}

/// CK *WaitGameTime - Utility*: "Pauses the script for at least the specified amount of game time":
/// the run suspends until the game clock has advanced by the wait, however little real time passes.
#[test]
fn wait_game_time_resumes_when_the_game_clock_reaches_it() {
    const SOURCE: &str = "ScriptName QF_VmWaitGame extends Quest\n\
        Function Fragment_1()\n\
          Utility.WaitGameTime(2.0)\n\
          Self.SetStage(6)\n\
        EndFunction\n";
    let (script, errors) = byroredux_papyrus::parse_script(SOURCE).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let script = Arc::new(script);
    if let Err(why) = crate::fragment::vm::checker::check_function_reason(Arc::clone(&script), "Fragment_1", None) {
        panic!("{why}")
    }
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    let mut vmad = vmad();
    vmad.scripts[0].name = "QF_VmWaitGame".into();
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        fragments.insert_vmad(Q, vmad);
        fragments.insert_vm_script(Arc::clone(&script));
        fragments.insert(
            Q,
            0,
            vec![Effect::Interpret { script: "QF_VmWaitGame".into(), function: "Fragment_1".into(), args: Vec::new(), resume: None }],
        );
    }
    crate::fragment::script_update_system(&world, 0.0, Some(10.0));
    world.resource_mut::<QuestStageState>().set_stage(Q, 0);
    crate::fragment::quest_fragment_dispatch_system(&world);
    crate::fragment::fragment_continuation_system(&world, 30.0);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 0, "real time does not end a game-time wait");
    crate::fragment::script_update_system(&world, 0.0, Some(11.0));
    crate::fragment::fragment_continuation_system(&world, 0.0);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 0, "an hour in");
    crate::fragment::script_update_system(&world, 0.0, Some(12.0));
    crate::fragment::fragment_continuation_system(&world, 0.0);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 6, "two game hours on, the run resumes");
}

/// #7298 — a VM `Message.Show()` runs under the quest guards, so it takes only the ticket there
/// (the catalog and the answers, two leaves); the box, whose text reads `Globals` and the alias
/// registry (the story manager takes that registry before `QuestStageState`), is built with the
/// deferred messages after the guards drop. A `Show` that builds its box inline is the regression.
#[test]
fn message_show_takes_its_ticket_under_the_guards_and_defers_the_box() {
    let source = include_str!("interp.rs");
    let body = source.split_once("fn message_show(").expect("message_show").1;
    let body = &body[..body.find("\n    }\n").expect("message_show ends")];
    assert!(body.contains("crate::messages::reserve_papyrus_box(self.world, form)"), "reserves the ticket");
    assert!(body.contains("self.deferred.defer_awaiting_box(form, numbers, ticket)"), "defers the box");
    for inline in ["show_message_awaiting(", "show_reserved_box(", "queue_message(", "replace_tags("] {
        assert!(!body.contains(inline), "message_show must not build its box under the guards: {inline}");
    }
}

/// #7290 — a function whose body waits on a box is a waiting function: called for its value it is
/// refused ahead of time, as one holding a `Utility.Wait` is, instead of raising the box and then
/// stopping at run time; called as a statement it runs. A box wrapper — a body of one `Return`
/// (`Return Question.Show()`, SkyUI's `ShowMessage`) — may be called for its value: its box is the
/// calling statement's, which replays with the answer (one box per statement still).
#[test]
fn a_function_that_shows_a_box_is_a_wait_for_its_callers() {
    const WRAPPER: &str = "ScriptName QF_VmAskWrapped extends Quest\n\
        Message Property Question Auto\n\
        Int Function Ask()\n\
          Return Question.Show()\n\
        EndFunction\n\
        Function Fragment_1()\n\
          If Ask() == 0\n\
            Self.SetStage(6)\n\
          EndIf\n\
        EndFunction\n\
        Function Fragment_2()\n\
          If Ask() == Ask()\n\
            Self.SetStage(6)\n\
          EndIf\n\
        EndFunction\n";
    let (wrapped, errors) = byroredux_papyrus::parse_script(WRAPPER).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let wrapped = Arc::new(wrapped);
    assert!(check_function_reason(Arc::clone(&wrapped), "Fragment_1", None).is_ok(), "a wrapper for its value");
    let two = check_function_reason(wrapped, "Fragment_2", None).err().unwrap_or_default();
    assert!(two.contains("two Message.Show() answers in one statement"), "{two}");

    const SOURCE: &str = "ScriptName QF_VmAsk extends Quest\n\
        Message Property Question Auto\n\
        Int Function Ask()\n\
          Int answer = Question.Show()\n\
          Return answer\n\
        EndFunction\n\
        Function Fragment_1()\n\
          If Ask() == 0\n\
            Self.SetStage(6)\n\
          EndIf\n\
        EndFunction\n\
        Function Fragment_2()\n\
          Ask()\n\
        EndFunction\n";
    let (script, errors) = byroredux_papyrus::parse_script(SOURCE).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let script = Arc::new(script);
    let reason = check_function_reason(Arc::clone(&script), "Fragment_1", None).err().unwrap_or_default();
    assert!(reason.contains("a wait in a function called for its value"), "{reason}");
    assert!(check_function_reason(script, "Fragment_2", None).is_ok(), "as a statement it may wait");
}

/// #7299 — a waited-on box replays its statement with the one answer it returned, so a statement
/// holding two `Show()` calls is refused ahead of time; one in an `If` and another in its body are
/// two statements and run.
#[test]
fn one_statement_holds_at_most_one_waited_box() {
    let check = |body: &str| {
        let source = format!(
            "ScriptName QF_VmTwo extends Quest\nMessage Property First Auto\nMessage Property Second Auto\n\
             Function Fragment_1()\n{body}\nEndFunction\n"
        );
        let (script, errors) = byroredux_papyrus::parse_script(&source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        check_function_reason(Arc::new(script), "Fragment_1", None)
    };
    let reason = check("If First.Show() == 0 && Second.Show() == 1\n Self.SetStage(6)\nEndIf").err().unwrap_or_default();
    assert!(reason.contains("two Message.Show() answers in one statement"), "{reason}");
    assert!(check("If First.Show() == 0\n Int b = Second.Show()\nEndIf").is_ok(), "one per statement");
}

/// #7299 — a `While` whose condition waits on a box, re-evaluated after a body that waited, suspends
/// on that box (the resumed path re-runs only the condition with the answer) instead of the run
/// stopping: the loop goes round once on button 1 and leaves on button 0.
#[test]
fn a_while_condition_waits_on_its_box_after_a_resumed_body() {
    const MESSAGE: u32 = 0x0001_0F10;
    const SOURCE: &str = "ScriptName QF_VmLoop extends Quest\n\
        Message Property Again Auto\n\
        Function Fragment_1()\n\
          Int rounds = 0\n\
          While Again.Show() == 1\n\
            rounds += 1\n\
            Utility.Wait(1.0)\n\
          EndWhile\n\
          Self.SetStage(10 + rounds)\n\
        EndFunction\n";
    let (script, errors) = byroredux_papyrus::parse_script(SOURCE).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let script = Arc::new(script);
    if let Err(why) = check_function_reason(Arc::clone(&script), "Fragment_1", None) { panic!("{why}") }
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    let mut index = byroredux_plugin::esm::records::EsmIndex::default();
    index.messages.insert(
        MESSAGE,
        byroredux_plugin::esm::records::MesgRecord {
            form_id: MESSAGE,
            description: "Again?".into(),
            message_box: true,
            buttons: vec!["No".into(), "Yes".into()],
            ..Default::default()
        },
    );
    crate::install_message_catalog(&mut world, &index);
    let mut vmad = vmad();
    vmad.scripts[0].name = "QF_VmLoop".into();
    vmad.scripts[0].properties.push(ScriptProperty {
        name: "Again".into(),
        status: 1,
        value: PropertyValue::Object { form_id: MESSAGE, alias: -1 },
    });
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        fragments.insert_vmad(Q, vmad);
        fragments.insert_vm_script(Arc::clone(&script));
        fragments.insert(
            Q,
            0,
            vec![Effect::Interpret { script: "QF_VmLoop".into(), function: "Fragment_1".into(), args: Vec::new(), resume: None }],
        );
    }
    let ticket = |world: &World| {
        let raised = std::mem::take(&mut world.resource_mut::<crate::PendingScriptMessages>().0);
        assert_eq!(raised.len(), 1, "one box is up");
        let crate::MessageAnswer::Papyrus(ticket) = raised[0].answer else { panic!("{:?}", raised[0].answer) };
        ticket
    };
    world.resource_mut::<QuestStageState>().set_stage(Q, 0);
    crate::fragment::quest_fragment_dispatch_system(&world);
    crate::messages::record_papyrus_answer(&world, ticket(&world), 1);
    crate::fragment::fragment_continuation_system(&world, 0.0); // round 1: the body waits
    crate::fragment::fragment_continuation_system(&world, 1.0); // the wait ends; the condition asks again
    let second = ticket(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 0, "the run waits on the second box");
    crate::messages::record_papyrus_answer(&world, second, 0);
    crate::fragment::fragment_continuation_system(&world, 0.0);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 11, "one round, then out");
}

/// #7289 — an answered `Message.Show()` run that is ready in a frame whose budget an earlier ready
/// run spends goes back on the queue. It already took its answer, so it must come back due, not
/// asking again: asking again read the lost-box default, button 0, in place of the one pressed.
#[test]
fn an_answered_show_deferred_by_the_budget_keeps_the_button_pressed() {
    const MESSAGE: u32 = 0x0001_0F10;
    const Q2: QuestFormId = QuestFormId(0x0001_0F01);
    const SHOW: &str = "ScriptName QF_VmShow extends Quest\n\
        Message Property TempChooseSidesMessage Auto\n\
        Function Fragment_1()\n\
          Utility.Wait(1.0)\n\
          If TempChooseSidesMessage.Show() == 0\n\
            Self.SetStage(6)\n\
          Else\n\
            Self.SetStage(7)\n\
          EndIf\n\
        EndFunction\n";
    // Resumes in the same frame as the answered run, queued ahead of it, and spends the budget.
    const BUSY: &str = "ScriptName QF_VmBusy extends Quest\n\
        Function Fragment_1()\n\
          Utility.Wait(6.0)\n\
          Int i = 0\n\
          While i < 400\n\
            i += 1\n\
          EndWhile\n\
        EndFunction\n";
    let parse = |source: &str, function: &str| {
        let (script, errors) = byroredux_papyrus::parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let script = Arc::new(script);
        if let Err(why) = crate::fragment::vm::checker::check_function_reason(Arc::clone(&script), function, None) { panic!("{why}") }
        script
    };
    let (show, busy) = (parse(SHOW, "Fragment_1"), parse(BUSY, "Fragment_1"));
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    let mut index = byroredux_plugin::esm::records::EsmIndex::default();
    index.messages.insert(
        MESSAGE,
        byroredux_plugin::esm::records::MesgRecord {
            form_id: MESSAGE,
            description: "Who will you follow?".into(),
            message_box: true,
            buttons: vec!["Hadvar".into(), "Ralof".into()],
            ..Default::default()
        },
    );
    crate::install_message_catalog(&mut world, &index);
    let mut show_vmad = vmad();
    show_vmad.scripts[0].name = "QF_VmShow".into();
    show_vmad.scripts[0].properties.push(ScriptProperty {
        name: "TempChooseSidesMessage".into(),
        status: 1,
        value: PropertyValue::Object { form_id: MESSAGE, alias: -1 },
    });
    let mut busy_vmad = vmad();
    busy_vmad.scripts[0].name = "QF_VmBusy".into();
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        for (quest, vmad, script, name) in [(Q2, busy_vmad, &busy, "QF_VmBusy"), (Q, show_vmad, &show, "QF_VmShow")] {
            fragments.insert_vmad(quest, vmad);
            fragments.insert_vm_script(Arc::clone(script));
            fragments.insert(
                quest,
                0,
                vec![Effect::Interpret { script: name.into(), function: "Fragment_1".into(), args: Vec::new(), resume: None }],
            );
        }
    }
    world.resource_mut::<QuestStageState>().set_stage(Q2, 0);
    world.resource_mut::<QuestStageState>().set_stage(Q, 0);
    crate::fragment::quest_fragment_dispatch_system(&world);
    crate::fragment::fragment_continuation_system(&world, 1.0);
    let raised = std::mem::take(&mut world.resource_mut::<crate::PendingScriptMessages>().0);
    let crate::MessageAnswer::Papyrus(ticket) = raised[0].answer else { panic!("{:?}", raised[0].answer) };
    crate::messages::record_papyrus_answer(&world, ticket, 1);

    // The frame both resume in: the busy run, ahead in the queue, spends a small budget.
    world.insert_resource(crate::fragment::budget::VmFrameBudget::with_limit(48));
    crate::fragment::fragment_continuation_system(&world, 5.0);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 0, "the answered run is deferred, not run");
    world.insert_resource(crate::fragment::budget::VmFrameBudget::default());
    crate::fragment::fragment_continuation_system(&world, 0.0);
    crate::fragment::fragment_continuation_system(&world, 0.0);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 7, "it keeps button 1, Ralof");
}

/// LMC-UI-002 slice 4 — an MCM Helper config script world: the script extends the engine's
/// `MCM_ConfigBase`, its quest belongs to `Frost.esp`, and the store reads `files`.
fn mcm_helper_world(config_source: &str, files: crate::mcm::helper::settings::tests::MemoryFiles) -> World {
    use crate::mcm::helper::McmHelperStore;
    let config = parsed_source(config_source);
    let scripts: HashMap<String, Arc<Script>> = [
        (crate::mcm::CONFIG_BASE.to_owned(), crate::mcm::engine_script("SKI_ConfigBase").expect("SKI_ConfigBase parses")),
        (crate::mcm::helper::HELPER_BASE.to_owned(), crate::mcm::engine_script("MCM_ConfigBase").expect("MCM_ConfigBase parses")),
        ("mcm".to_owned(), crate::mcm::engine_script("MCM").expect("MCM parses")),
    ]
    .into();
    let library = Installed(&scripts);
    let extends = vec![crate::mcm::CONFIG_BASE.to_owned(), crate::mcm::helper::HELPER_BASE.to_owned()];
    let mut world = world_with_quest_script_extending(Arc::clone(&config), Some(&library), true, extends)
        .unwrap_or_else(|| panic!("declined: {:?}", crate::check_quest_script(config, &crate::DELIVERED_QUEST_EVENTS, Some(&library)).err()));
    world.insert_resource(crate::load_order::LoadOrderIdentity::new(vec![(
        byroredux_plugin::esm::reader::GlobalSlot::Regular(0),
        PluginId::from_filename("Frost.esp"),
    )]));
    // The quest is a quest with scripts, so its properties can back a control.
    let instance = ScriptInstanceData { version: 5, object_format: 2, scripts: Vec::new() };
    world.insert_resource(crate::QuestScriptInstances(Arc::new([(Q, instance)].into())));
    world.insert_resource(McmHelperStore::new(Arc::new(files), &["Skyrim.esm".to_owned(), "Frost.esp".to_owned()]));
    world
}

const HELPER_SCRIPT: &str = "ScriptName VmHelperConfig extends MCM_ConfigBase\n\
     Int Property Speed Auto\n\
     Int Property Changes Auto\n\
     Float Property RateSeen Auto\n\
     Int Property Applied Auto\n\
     Event OnSettingChange(String a_ID)\n\
      Changes += 1\n\
      If a_ID == \"fRate:Main\"\n RateSeen = GetModSettingFloat(\"fRate:Main\")\n EndIf\n\
     EndEvent\n\
     Function Apply(Bool a_value, Int a_tag)\n\
      If a_value\n Applied = a_tag\n Else\n Applied = -a_tag\n EndIf\n\
     EndFunction\n";

const HELPER_CONFIG: &str = r#"{
    "modName": "Frost",
    "displayName": "Frost Config",
    "pluginRequirements": ["Frost.esp"],
    "content": [
        { "type": "header", "text": "General" },
        { "id": "bEnabled:Main", "type": "toggle", "text": "Enabled", "groupControl": 1,
          "valueOptions": { "sourceType": "ModSettingBool" },
          "action": { "type": "CallFunction", "function": "Apply", "params": ["{value}", "{i}7"] } },
        { "id": "fRate:Main", "type": "slider", "text": "Rate", "help": "Rate is {value}", "groupCondition": 1,
          "valueOptions": { "min": 0, "max": 5, "step": 0.5, "formatString": "{1}x", "sourceType": "ModSettingFloat" } },
        { "type": "enum", "text": "Speed", "valueOptions": { "options": ["Slow", "Fast"], "sourceType": "PropertyValueInt", "propertyName": "Speed", "defaultValue": 0 } },
        { "id": "iMode:Main", "type": "stepper", "text": "Mode", "valueOptions": { "options": ["A", "B", "C"], "sourceType": "ModSettingInt" } },
        { "id": "iKey:Main", "type": "keymap", "text": "Hotkey", "valueOptions": { "sourceType": "ModSettingInt" } },
        { "id": "sName:Main", "type": "input", "text": "Name" }
    ],
    "pages": [ { "pageDisplayName": "Extra", "content": [ { "type": "text", "text": "Version", "valueOptions": { "value": "3.4" } } ] } ]
}"#;

fn helper_files(config: &str) -> crate::mcm::helper::settings::tests::MemoryFiles {
    let mut files = crate::mcm::helper::settings::tests::MemoryFiles::default();
    files.config.insert("Frost".into(), config.into());
    files.defaults.insert("Frost".into(), "[Main]\nbEnabled=1\nfRate=1.5\niMode=0\niKey=48\nsName=Ysolda\n".into());
    files
}

/// LMC-UI-002 slice 4 — an MCM Helper config script on the engine's `MCM_ConfigBase`: it registers with
/// the config's display name and pages, its page is laid out from `config.json` with each control's
/// value read where it lives, and the panel's actions write those values, raise `OnSettingChange` with
/// the control's id, follow the group switch and run the control's action.
#[test]
fn an_mcm_helper_config_lays_out_from_json_and_writes_where_values_live() {
    use crate::mcm::{McmAnswer, McmDialog, McmHost, McmKind};
    let files = helper_files(HELPER_CONFIG);
    let world = mcm_helper_world(HELPER_SCRIPT, files);
    crate::quest_script_init_system(&world, 0.0);
    {
        let host = world.resource::<McmHost>();
        assert_eq!(host.mods.len(), 1, "registered from OnInit");
        assert_eq!((host.mods[0].name.as_str(), host.mods[0].pages.as_slice()), ("Frost Config", &["Extra".to_owned()][..]));
    }
    crate::mcm::open(&world, 0).expect("opens");
    let cell = |world: &World, index: usize| world.resource::<McmHost>().open.as_ref().unwrap().cells[index].clone().unwrap();
    assert_eq!(cell(&world, 0).kind, McmKind::Header);
    assert_eq!((cell(&world, 1).kind, cell(&world, 1).number), (McmKind::Toggle, 1.0), "bEnabled's default");
    assert_eq!((cell(&world, 2).kind, cell(&world, 2).number, cell(&world, 2).flags), (McmKind::Slider, 1.5, 0));
    assert_eq!(cell(&world, 2).value_text(), "1.5x");
    assert_eq!((cell(&world, 3).kind, cell(&world, 3).text.as_str()), (McmKind::Menu, "Slow"));
    assert_eq!((cell(&world, 4).kind, cell(&world, 4).text.as_str()), (McmKind::Text, "A"));
    assert_eq!((cell(&world, 5).kind, cell(&world, 5).number, cell(&world, 5).flags), (McmKind::KeyMap, 48.0, flags_with_unmap()));
    assert_eq!((cell(&world, 6).kind, cell(&world, 6).text.as_str()), (McmKind::Input, "Ysolda"));

    // The toggle: its setting flips, the group it switches disables the slider, the script hears the
    // id, and the action runs with the new value.
    crate::mcm::activate(&world, 1).expect("selects");
    let variable = |world: &World, name: &str| quest_property(world, name);
    assert_eq!(cell(&world, 1).number, 0.0);
    assert_eq!(cell(&world, 2).flags, crate::mcm::flags::DISABLED, "group 1 is off");
    assert_eq!(variable(&world, "Changes"), Some(1.0), "OnSettingChange(\"bEnabled:Main\")");
    assert_eq!(variable(&world, "Applied"), Some(-7.0), "Apply(false, 7) ran after the event");
    crate::mcm::activate(&world, 1).expect("selects again");
    assert_eq!(cell(&world, 2).flags, 0, "group 1 is on again");
    assert_eq!(variable(&world, "Applied"), Some(7.0));

    // The slider: its dialog takes the setting, its default and range; the accepted value is written.
    crate::mcm::activate(&world, 2).expect("opens the dialog");
    assert_eq!(
        world.resource::<McmHost>().open.as_ref().unwrap().dialog,
        Some((2, McmDialog::Slider { start: 1.5, default: 1.5, min: 0.0, max: 5.0, interval: 0.5 }))
    );
    crate::mcm::accept(&world, 2, McmAnswer::Slider(2.5)).expect("accepts");
    assert_eq!(cell(&world, 2).value_text(), "2.5x");
    assert_eq!(variable(&world, "RateSeen"), Some(2.5), "the script read the new value in OnSettingChange");
    crate::mcm::highlight(&world, 2).expect("highlights");
    assert_eq!(world.resource::<McmHost>().open.as_ref().unwrap().info, "Rate is 2.5", "help with {{value}}");
    crate::mcm::reset_option(&world, 2).expect("resets");
    assert_eq!(cell(&world, 2).number, 1.5, "back to the settings.ini default");

    // The enum writes the script property; the stepper steps its setting; the key map and input store theirs.
    crate::mcm::activate(&world, 3).expect("opens the menu");
    assert!(matches!(&world.resource::<McmHost>().open.as_ref().unwrap().dialog, Some((3, McmDialog::Menu { options, start: 0, .. })) if options == &["Slow", "Fast"]));
    crate::mcm::accept(&world, 3, McmAnswer::Menu(1)).expect("accepts");
    assert_eq!(variable(&world, "Speed"), Some(1.0));
    assert_eq!(cell(&world, 3).text, "Fast");
    crate::mcm::activate(&world, 4).expect("steps");
    assert_eq!(cell(&world, 4).text, "B");
    crate::mcm::activate(&world, 5).expect("opens the key prompt");
    crate::mcm::accept(&world, 5, McmAnswer::KeyMap(0x22, String::new())).expect("binds");
    assert_eq!(cell(&world, 5).number, 34.0);
    crate::mcm::activate(&world, 6).expect("opens the input");
    assert_eq!(world.resource::<McmHost>().open.as_ref().unwrap().dialog, Some((6, McmDialog::Input { start_text: "Ysolda".into() })));
    crate::mcm::accept(&world, 6, McmAnswer::Input("Lydia".into())).expect("accepts");
    assert_eq!(cell(&world, 6).text, "Lydia");

    // Every write reached the profile's file, and only the changed keys.
    let store = world.resource::<crate::mcm::helper::McmHelperStore>();
    drop(store);
    let written = {
        let mut store = world.resource_mut::<crate::mcm::helper::McmHelperStore>();
        (store.setting_bool("Frost", "bEnabled:Main"), store.setting_float("Frost", "fRate:Main"), store.setting_int("Frost", "iMode:Main"), store.setting_text("Frost", "sName:Main"))
    };
    assert_eq!(written, (true, 1.5, 1, "Lydia".to_owned()));

    // The sub-page.
    crate::mcm::select_page(&world, Some(0)).expect("shows the page");
    assert_eq!((cell(&world, 0).kind, cell(&world, 0).text.as_str()), (McmKind::Text, "3.4"));
}

fn flags_with_unmap() -> u32 {
    crate::mcm::flags::WITH_UNMAP
}

/// A config that does not load shows MCM Helper's error option, whose highlight explains why; so does
/// one needing a later MCM Helper or a plugin the session lacks.
#[test]
fn an_mcm_helper_config_that_cannot_load_shows_the_error_page() {
    use crate::mcm::{McmHost, McmKind};
    for (config, reason) in [
        ("{ \"content\": [ { \"type\": \"dial\" } ] }", "unknown control type 'dial'"),
        (r#"{ "minMcmVersion": 99, "content": [] }"#, "version code 99"),
        (r#"{ "pluginRequirements": ["Missing.esp"], "content": [] }"#, "Missing.esp"),
    ] {
        let world = mcm_helper_world(HELPER_SCRIPT, helper_files(config));
        crate::quest_script_init_system(&world, 0.0);
        crate::mcm::open(&world, 0).expect("opens");
        let option = world.resource::<McmHost>().open.as_ref().unwrap().cells[0].clone().expect("the error option");
        assert_eq!((option.kind, option.label.as_str(), option.text.as_str()), (McmKind::Text, "Error loading config", "Click to view"));
        crate::mcm::highlight(&world, 0).expect("highlights");
        let info = world.resource::<McmHost>().open.as_ref().unwrap().info.clone();
        assert!(info.contains(reason), "{info:?} names {reason:?}");
    }
}

/// `MCM.IsInstalled()` / `GetVersionCode()` answer a mod probing for MCM Helper, and the global setting
/// calls read and write another mod's settings by name.
#[test]
fn mcm_helpers_global_api_answers_probes_and_settings() {
    let world = mcm_helper_world(
        "ScriptName VmHelperProbe extends MCM_ConfigBase\n\
         Int Property Code Auto\n\
         Int Property Mode Auto\n\
         Event OnConfigInit()\n\
          If MCM.IsInstalled()\n Code = MCM.GetVersionCode()\n EndIf\n\
          MCM.SetModSettingInt(\"Frost\", \"iMode:Main\", 2)\n\
          Mode = MCM.GetModSettingInt(\"Frost\", \"iMode:Main\") + GetModSettingInt(\"iMode:Main\")\n\
         EndEvent\n",
        helper_files(HELPER_CONFIG),
    );
    crate::quest_script_init_system(&world, 0.0);
    assert_eq!(quest_property(&world, "Code"), Some(crate::mcm::helper::VERSION_CODE as f32));
    assert_eq!(quest_property(&world, "Mode"), Some(4.0), "both reads see the write");
}

/// LMC-UI-002 slice 4b — MCM Helper's hotkeys: a key-map control with an id binds a key to the hotkey
/// of `keybinds.json` the id names (saved in the profile's bindings, over the bindings an earlier
/// install left); a press runs a function call, `SendEvent` delivers `OnControlDown` and, on release,
/// `OnControlUp` with the time held; a console-command hotkey is declined; an unmapped key stops it.
#[test]
fn mcm_helper_hotkeys_bind_through_a_key_map_and_run_on_a_press() {
    use crate::mcm::helper::{hotkey_events, McmHelperStore};
    use crate::mcm::{McmAnswer, McmHost};
    let mut files = helper_files(
        r#"{ "modName": "Frost", "displayName": "Frost Config",
             "content": [ { "id": "warm", "type": "keymap", "text": "Warm up" } ] }"#,
    );
    files.keybinds.insert(
        "Frost".into(),
        r#"{ "modName": "Frost", "keybinds": [
            { "id": "warm", "desc": "Warm yourself", "action": { "type": "CallFunction", "function": "Apply", "params": [true, "{i}5"] } },
            { "id": "sprint", "action": { "type": "SendEvent", "form": "Frost.esp|10F00", "scriptName": "VmHotkeyConfig" } },
            { "id": "tgm", "action": { "type": "RunConsoleCommand", "command": "tgm" } }
        ] }"#
            .into(),
    );
    files.legacy_registrations = Some(
        r#"{ "version": 15, "keybinds": [ { "keycode": 42, "modName": "Frost", "id": "sprint" }, { "keycode": 20, "modName": "Frost", "id": "tgm" } ] }"#.into(),
    );
    let files = std::sync::Arc::new(files);
    let mut world = mcm_helper_world(
        "ScriptName VmHotkeyConfig extends MCM_ConfigBase\n\
         Int Property Applied Auto\n\
         Int Property Downs Auto\n\
         Float Property Held Auto\n\
         Function Apply(Bool a_value, Int a_tag)\n If a_value\n Applied += a_tag\n EndIf\n EndFunction\n\
         Event OnControlDown(String control)\n If control == \"sprint\"\n Downs += 1\n EndIf\n EndEvent\n\
         Event OnControlUp(String control, Float holdTime)\n Held = holdTime\n EndEvent\n",
        crate::mcm::helper::settings::tests::MemoryFiles::default(),
    );
    world.insert_resource(McmHelperStore::new(files.clone(), &["Skyrim.esm".to_owned(), "Frost.esp".to_owned()]));
    crate::quest_script_init_system(&world, 0.0);
    crate::mcm::open(&world, 0).expect("opens");
    let key = |world: &World| world.resource::<McmHost>().open.as_ref().unwrap().cells[0].as_ref().unwrap().number;
    assert_eq!(key(&world), -1.0, "nothing bound yet");
    crate::mcm::highlight(&world, 0).expect("highlights");
    assert_eq!(world.resource::<McmHost>().open.as_ref().unwrap().info, "Warm yourself", "the hotkey's description");

    crate::mcm::activate(&world, 0).expect("opens the key prompt");
    crate::mcm::accept(&world, 0, McmAnswer::KeyMap(0x22, String::new())).expect("binds");
    assert_eq!(key(&world), 34.0);
    let written = files.profile_registrations.lock().unwrap().clone().expect("the profile's bindings were written");
    let bindings = crate::mcm::helper::keybinds::parse_registrations(written.as_bytes());
    assert_eq!(bindings.len(), 3, "the earlier install's two bindings carried over: {written}");
    crate::mcm::close(&world).expect("closes");

    let applied = |world: &World| quest_property(world, "Applied");
    hotkey_events(&world, &[0x22], &[]);
    assert_eq!(applied(&world), Some(5.0), "Apply(true, 5) ran on the press");
    hotkey_events(&world, &[], &[(0x22, 0.4)]);
    assert_eq!(applied(&world), Some(5.0), "a function call runs on the key going down only");
    hotkey_events(&world, &[42], &[]);
    assert_eq!(quest_property(&world, "Downs"), Some(1.0), "OnControlDown(\"sprint\")");
    hotkey_events(&world, &[], &[(42, 1.5)]);
    assert_eq!(quest_property(&world, "Held"), Some(1.5), "OnControlUp with the time held");
    hotkey_events(&world, &[20], &[]);
    hotkey_events(&world, &[0x30], &[]);
    assert_eq!(applied(&world), Some(5.0), "the console hotkey and an unbound key run nothing");

    crate::mcm::open(&world, 0).expect("opens again");
    crate::mcm::activate(&world, 0).expect("opens the key prompt");
    crate::mcm::accept(&world, 0, McmAnswer::KeyMap(-1, String::new())).expect("unmaps");
    hotkey_events(&world, &[0x22], &[]);
    assert_eq!(applied(&world), Some(5.0), "the unmapped hotkey no longer runs");
}

/// LMC-UI-002 slice 5 — Fallout 4 MCM: a config no script registers is listed by its folder when its
/// plugin is loaded (one whose plugin is missing is not), its page is laid out by FO4's control names, a
/// switcher's group hides the controls it gates, a button runs its action, an image is left out, and a
/// quest script registered with F4SE's `RegisterForExternalEvent` hears the opening, each change (with
/// the mod and the id) and the closing.
#[test]
fn a_fallout4_mcm_config_registers_itself_and_sends_external_events() {
    use crate::mcm::helper::McmHelperStore;
    use crate::mcm::{McmAnswer, McmHost, McmKind};
    let mut files = helper_files(
        r#"{ "modName": "Frost", "displayName": "Frost Config",
             "content": [
                { "type": "section", "text": "General" },
                { "id": "bEnabled:Main", "type": "switcher", "text": "Enabled", "groupControl": 1, "valueOptions": { "sourceType": "ModSettingBool" } },
                { "id": "fRate:Main", "type": "slider", "text": "Rate", "groupCondition": 1, "valueOptions": { "min": 0, "max": 5, "step": 0.25, "sourceType": "ModSettingFloat" } },
                { "type": "dropdown", "text": "Speed", "valueOptions": { "options": ["Slow", "Fast"], "sourceType": "PropertyValueInt", "sourceForm": "Frost.esp|10F00", "propertyName": "Speed" } },
                { "type": "button", "text": "Apply", "action": { "type": "CallFunction", "form": "Frost.esp|10F00", "function": "Applied", "params": ["{i}3"] } },
                { "type": "image", "libName": "lib", "className": "Logo" },
                { "type": "text", "text": "Version 1.0", "id": "unused" }
             ] }"#,
    );
    files.config.insert("Ghost".into(), r#"{ "modName": "Ghost", "displayName": "Ghost", "content": [] }"#.into());
    let mut world = mcm_helper_world(
        "ScriptName VmFo4Listener extends Quest\n\
         Int Property Speed Auto\n\
         Int Property Changes Auto\n\
         Int Property Opens Auto\n\
         Int Property Closes Auto\n\
         Int Property Button Auto\n\
         Int Property RateChanges Auto\n\
         Event OnInit()\n\
          RegisterForExternalEvent(\"OnMCMSettingChange|Frost\", \"OnSetting\")\n\
          RegisterForExternalEvent(\"OnMCMMenuOpen\", \"OnOpen\")\n\
          RegisterForExternalEvent(\"OnMCMMenuClose|Frost\", \"OnClose\")\n\
         EndEvent\n\
         Function OnSetting(String modName, String id)\n\
          If modName == \"Frost\"\n Changes += 1\n EndIf\n\
          If id == \"fRate:Main\"\n RateChanges += 1\n EndIf\n\
         EndFunction\n\
         Function OnOpen(String modName)\n Opens += 1\n EndFunction\n\
         Function OnClose(String modName)\n Closes += 1\n EndFunction\n\
         Function Applied(Int a_tag)\n Button = a_tag\n EndFunction\n",
        crate::mcm::helper::settings::tests::MemoryFiles::default(),
    );
    world.insert_resource(
        McmHelperStore::new(std::sync::Arc::new(files), &["Fallout4.esm".to_owned(), "Frost.esp".to_owned()])
            .for_fallout4(vec!["Frost".into(), "Ghost".into()]),
    );
    crate::quest_script_init_system(&world, 0.0);
    crate::mcm::mcm_registration_system(&world, 0.0);
    {
        let host = world.resource::<McmHost>();
        assert_eq!(host.mods.len(), 1, "Ghost.esp is not loaded, so its config is not listed");
        assert_eq!((host.mods[0].name.as_str(), host.mods[0].config.as_deref()), ("Frost Config", Some("Frost")));
    }
    assert_eq!(world.resource::<McmHelperStore>().version_code(), 9, "FO4 MCM 1.40's version code");

    crate::mcm::open(&world, 0).expect("opens");
    assert_eq!(quest_property(&world, "Opens"), Some(1.0), "OnMCMMenuOpen(\"Frost\")");
    let cell = |world: &World, index: usize| world.resource::<McmHost>().open.as_ref().unwrap().cells[index].clone();
    let kinds: Vec<_> = (0..7).map(|i| cell(&world, i).map(|c| c.kind)).collect();
    assert_eq!(
        kinds,
        [Some(McmKind::Header), Some(McmKind::Toggle), Some(McmKind::Slider), Some(McmKind::Menu), Some(McmKind::Text), Some(McmKind::Text), None],
        "the image is left out"
    );
    assert_eq!(cell(&world, 2).unwrap().value_text(), "1.50", "a slider shows its step's decimals");
    assert_eq!(cell(&world, 3).unwrap().text, "Slow");
    assert_eq!((cell(&world, 5).unwrap().label.as_str(), cell(&world, 5).unwrap().text.as_str()), ("Version 1.0", ""), "FO4's text has no value");

    crate::mcm::activate(&world, 1).expect("switches");
    assert_eq!(cell(&world, 2).unwrap().flags, crate::mcm::flags::DISABLED | crate::mcm::flags::HIDDEN, "a failed group hides on FO4");
    assert_eq!(quest_property(&world, "Changes"), Some(1.0), "OnMCMSettingChange(\"Frost\", \"bEnabled:Main\")");
    crate::mcm::activate(&world, 1).expect("switches back");
    crate::mcm::activate(&world, 2).expect("opens the slider");
    crate::mcm::accept(&world, 2, McmAnswer::Slider(3.25)).expect("accepts");
    assert_eq!(quest_property(&world, "RateChanges"), Some(1.0), "the id reaches the callback");
    assert_eq!(quest_property(&world, "Changes"), Some(3.0));

    crate::mcm::activate(&world, 3).expect("opens the dropdown");
    crate::mcm::accept(&world, 3, McmAnswer::Menu(1)).expect("accepts");
    assert_eq!(quest_property(&world, "Speed"), Some(1.0), "the dropdown's index into the property");
    assert_eq!(cell(&world, 3).unwrap().text, "Fast");

    crate::mcm::activate(&world, 4).expect("presses the button");
    assert_eq!(quest_property(&world, "Button"), Some(3.0), "the button's CallFunction ran");

    crate::mcm::close(&world).expect("closes");
    assert_eq!(quest_property(&world, "Closes"), Some(1.0), "OnMCMMenuClose|Frost");

    // A load clears the host and the session's registrations; the config lists itself again.
    world.resource_mut::<McmHost>().reset_session();
    crate::fragment::reset_external_events(&world);
    crate::mcm::mcm_registration_system(&world, 0.0);
    assert_eq!(world.resource::<McmHost>().mods.len(), 1);
    assert!(world.resource::<crate::fragment::ExternalEventRegistrations>().0.is_empty());
}

/// F4SE's `RegisterForExternalEvent` is checked like the script's own calls: a callback the script lacks,
/// or one named by a variable, declines it.
#[test]
fn register_for_external_event_needs_a_literal_callback_the_script_has() {
    let check = |source: &str| crate::check_quest_script(parsed_source(source), &crate::DELIVERED_QUEST_EVENTS, None);
    assert!(check(
        "ScriptName VmExt1 extends Quest\n\
         Event OnInit()\n RegisterForExternalEvent(\"OnMCMSettingChange\", \"Cb\")\n UnregisterForExternalEvent(\"Other\")\n EndEvent\n\
         Function Cb(String a, String b)\n EndFunction\n"
    )
    .is_ok());
    assert!(check("ScriptName VmExt2 extends Quest\nEvent OnInit()\n RegisterForExternalEvent(\"E\", \"Missing\")\n EndEvent\n").is_err());
    assert!(check(
        "ScriptName VmExt3 extends Quest\nString cb = \"Cb\"\nEvent OnInit()\n RegisterForExternalEvent(\"E\", cb)\n EndEvent\nFunction Cb()\nEndFunction\n"
    )
    .is_err());
}

/// LMC-SCR-010: `SKSE`'s version probes answer the level of the mod the calling script came from — here a mod
/// whose scripts call nothing unserved, so the newest documented release — with `GetScriptVersionRelease` the same
/// release (no script/runtime mismatch) and every plugin not loaded; a script no source holds gets "absent".
#[test]
fn skse_version_probes_answer_the_calling_mods_level() {
    use crate::extender_catalog::{probe_answer_for, ExtenderProbes};
    use crate::script_sources::{ScriptSourceIndex, ScriptSources};
    struct OneMod;
    impl ScriptSources for OneMod {
        fn source_of(&self, _: &World, script: &str) -> Option<usize> {
            script.eq_ignore_ascii_case("QF_VmProbe").then_some(0)
        }
        fn scripts(&self, _: &World, _: usize) -> Vec<Vec<u8>> {
            Vec::new()
        }
        fn principal_of(&self, _: &World, _: &str) -> Option<byroredux_sdk::identity::PrincipalId> {
            None
        }
    }
    let script = parsed_source(
        "ScriptName QF_VmProbe extends Quest\n\
         Int Property Major Auto\n\
         Int Property Minor Auto\n\
         Int Property Beta Auto\n\
         Int Property Release Auto\n\
         Int Property ScriptRelease Auto\n\
         Int Property Plugin Auto\n\
         Function Fragment_1()\n\
           Major = SKSE.GetVersion()\n\
           Minor = SKSE.GetVersionMinor()\n\
           Beta = SKSE.GetVersionBeta()\n\
           Release = SKSE.GetVersionRelease()\n\
           ScriptRelease = SKSE.GetScriptVersionRelease()\n\
           Plugin = SKSE.GetPluginVersion(\"JContainers64\")\n\
         EndFunction\n",
    );
    if let Err(why) = crate::fragment::vm::checker::check_function_reason(Arc::clone(&script), "Fragment_1", None) {
        panic!("{why}")
    }
    let mut world = World::new();
    crate::register(&mut world);
    world.insert_resource(QuestStageState::default());
    world.insert_resource(crate::QuestScriptVariables::default());
    world.insert_resource(crate::QuestScriptObjects::default());
    world.insert_resource(ScriptSourceIndex(Box::new(OneMod)));
    world.insert_resource(ExtenderProbes::new("skyrim"));
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    let vmad = ScriptInstanceData {
        version: 5,
        object_format: 2,
        scripts: vec![ScriptInstance { name: "QF_VmProbe".into(), status: 0, properties: Vec::new() }],
    };
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        fragments.insert_vmad(Q, vmad);
        fragments.insert_vm_script(Arc::clone(&script));
        fragments.insert(
            Q,
            10,
            vec![Effect::Interpret { script: "QF_VmProbe".into(), function: "Fragment_1".into(), args: Vec::new(), resume: None }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    let newest = probe_answer_for(&world, "skse64", "QF_VmProbe");
    let (major, minor, beta) = newest.triple();
    assert!(newest.level.is_some() && newest.release > 0, "{newest:?}");
    assert_eq!(quest_property(&world, "Major"), Some(major as f32));
    assert_eq!(quest_property(&world, "Minor"), Some(minor as f32));
    assert_eq!(quest_property(&world, "Beta"), Some(beta as f32));
    assert_eq!(quest_property(&world, "Release"), Some(newest.release as f32));
    assert_eq!(quest_property(&world, "ScriptRelease"), Some(newest.release as f32), "no script/runtime mismatch");
    assert_eq!(quest_property(&world, "Plugin"), Some(-1.0), "no extender plugin is loaded");
    // A script no source holds, and the other game's family: absent.
    assert_eq!(probe_answer_for(&world, "skse64", "SomeoneElse").level, None);
    assert_eq!(probe_answer_for(&world, "f4se", "QF_VmProbe").level, None);
}

/// LMC-SCR-004 catalog growth: SKSE's `StringUtil`, `Math` bit operations and `Utility` arrays run in the VM as a
/// mod script writes them (a settings string split into its parts, flags tested, an array built and grown).
#[test]
fn skse_string_math_and_array_natives_run_in_the_vm() {
    let script = parsed_source(
        "ScriptName QF_VmSkseUtil extends Quest\n\
         Int Property Results Auto\n\
         Function Fragment_1()\n\
           String[] parts = StringUtil.Split(\"Iron;Steel;;Ebony\", \";\")\n\
           If parts.Length == 3 && parts[2] == \"Ebony\"\n Results += 1\n EndIf\n\
           If StringUtil.Find(parts[1], \"EEL\") == 2 && StringUtil.Substring(parts[0], 1, 2) == \"ro\"\n Results += 10\n EndIf\n\
           Int flags = Math.LogicalOr(Math.LeftShift(1, 3), 1)\n\
           If Math.LogicalAnd(flags, 8) != 0 && flags == 9\n Results += 100\n EndIf\n\
           Int[] counts = Utility.CreateIntArray(2, 7)\n\
           counts = Utility.ResizeIntArray(counts, 4)\n\
           If counts.Length == 4 && counts[1] == 7 && counts[3] == 0\n Results += 1000\n EndIf\n\
           If StringUtil.GetLength(StringUtil.AsChar(65)) == 1 && StringUtil.IsDigit(\"4\")\n Results += 10000\n EndIf\n\
         EndFunction\n",
    );
    if let Err(why) = crate::fragment::vm::checker::check_function_reason(Arc::clone(&script), "Fragment_1", None) {
        panic!("{why}")
    }
    let mut world = world_with_fragment(script);
    world.insert_resource(crate::QuestScriptVariables::default());
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(quest_property(&world, "Results"), Some(11_111.0));
}

/// LMC-SCR-004 catalog growth: SKSE's `Form` reads on Skyrim. A weapon answers its record's type, name, weight,
/// keywords (null entries counted but not listed) and model; an armor's class has no world model; a door's type
/// and model presence come from its class; the player is a reference with none of the components. A door's name,
/// which its class has but the engine does not read, stops the call instead of answering "".
#[test]
fn skse_form_reads_answer_from_the_records_and_the_classes() {
    use byroredux_plugin::esm::records::{EsmIndex, ItemKind, ItemRecord};
    const SWORD: u32 = 0x100;
    const CUIRASS: u32 = 0x101;
    const DOOR: u32 = 0x102;
    let mut index = EsmIndex { game: byroredux_plugin::esm::reader::GameKind::Skyrim, ..EsmIndex::default() };
    let mut sword = ItemRecord { form_id: SWORD, common: Default::default(), kind: ItemKind::weapon_stub(8, 0) };
    sword.common.full_name = "Iron Sword".into();
    sword.common.weight = 9.0;
    sword.common.keywords = vec![0x200, 0, 0x201];
    sword.common.model_path = "Weapons\\Iron\\Sword.nif".into();
    index.items.insert(SWORD, sword);
    let mut cuirass = ItemRecord { form_id: CUIRASS, common: Default::default(), kind: ItemKind::weapon_stub(8, 0) };
    cuirass.common.full_name = "Iron Armor".into();
    index.items.insert(CUIRASS, cuirass);
    for (form, signature) in [(SWORD, b"WEAP"), (CUIRASS, b"ARMO"), (DOOR, b"DOOR"), (0x200, b"KYWD"), (0x201, b"KYWD"), (0x7, b"NPC_")] {
        index.record_types.insert(form, *signature);
    }
    let script = parsed_source(
        "ScriptName QF_VmSkseForm extends Quest\n\
         Form Property Sword Auto\n\
         Form Property Cuirass Auto\n\
         Form Property Door Auto\n\
         Int Property Results Auto\n\
         Function Fragment_1()\n\
           If Sword.GetType() == 41 && Sword.GetName() == \"Iron Sword\" && Sword.GetWeight() == 9.0\n Results += 1\n EndIf\n\
           If Sword.GetNumKeywords() == 3 && Sword.GetNthKeyword(1) == None && Sword.GetNthKeyword(-1) == None && Sword.GetKeywords().Length == 2\n Results += 10\n EndIf\n\
           If Sword.HasWorldModel() && StringUtil.Find(Sword.GetWorldModelPath(), \"Sword.nif\") > 0\n Results += 100\n EndIf\n\
           If Cuirass.GetType() == 26 && !Cuirass.HasWorldModel() && Cuirass.GetWorldModelPath() == \"\" && Door.GetType() == 29 && Door.HasWorldModel()\n Results += 1000\n EndIf\n\
           If Game.GetPlayer().GetType() == 62 && Game.GetPlayer().GetName() == \"\"\n Results += 10000\n EndIf\n\
         EndFunction\n\
         Function Fragment_2()\n\
           String name = Door.GetName()\n\
           Results += 100000\n\
         EndFunction\n",
    );
    for fragment in ["Fragment_1", "Fragment_2"] {
        if let Err(why) = crate::fragment::vm::checker::check_function_reason(Arc::clone(&script), fragment, None) {
            panic!("{why}")
        }
    }
    let mut world = World::new();
    crate::register(&mut world);
    world.insert_resource(crate::ScriptGame(byroredux_plugin::esm::reader::GameKind::Skyrim));
    world.insert_resource(crate::form_facts::FormFacts::from_index(&index));
    world.insert_resource(QuestStageState::default());
    world.insert_resource(crate::QuestScriptVariables::default());
    world.insert_resource(crate::QuestScriptObjects::default());
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    let object = |name: &str, form_id: u32| ScriptProperty { name: name.into(), status: 1, value: PropertyValue::Object { form_id, alias: -1 } };
    let vmad = ScriptInstanceData {
        version: 5,
        object_format: 2,
        scripts: vec![ScriptInstance {
            name: "QF_VmSkseForm".into(),
            status: 0,
            properties: vec![object("Sword", SWORD), object("Cuirass", CUIRASS), object("Door", DOOR)],
        }],
    };
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        fragments.insert_vmad(Q, vmad);
        fragments.insert_vm_script(Arc::clone(&script));
        for (stage, function) in [(10, "Fragment_1"), (20, "Fragment_2")] {
            fragments.insert(
                Q,
                stage,
                vec![Effect::Interpret { script: "QF_VmSkseForm".into(), function: function.into(), args: Vec::new(), resume: None }],
            );
        }
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(quest_property(&world, "Results"), Some(11_111.0), "every read answered");
    world.resource_mut::<QuestStageState>().set_stage(Q, 20);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(quest_property(&world, "Results"), Some(11_111.0), "a door's unread name stops the run");
}

/// LMC-SCR-004 catalog growth: SKSE's key, control and menu registrations. A quest script registers its object for
/// a key, a control and a menu; the host's transitions reach `OnKeyDown` / `OnKeyUp` (with the hold time),
/// `OnControlDown` and `OnMenuOpen` / `OnMenuClose`, compared without case; `UnregisterForAllKeys` stops the keys
/// while the control and menu stay. The `Input` reads answer the keys the host holds, in ascending code order.
#[test]
fn skse_key_control_and_menu_registrations_deliver_their_events() {
    use byroredux_core::ecs::resources::menu_registry::{MenuGame, MenuRegistry, MenuSurface};
    let script = parsed_source(
        "ScriptName QF_VmSkseInput extends Quest\n\
         Int Property Downs Auto\n\
         Int Property Ups Auto\n\
         Float Property Held Auto\n\
         Int Property Controls Auto\n\
         Int Property Menus Auto\n\
         Int Property Reads Auto\n\
         Function Fragment_1()\n\
           RegisterForKey(57)\n\
           RegisterForControl(\"Activate\")\n\
           RegisterForMenu(\"InventoryMenu\")\n\
           If Input.IsKeyPressed(30) && !Input.IsKeyPressed(57) && Input.GetNumKeysPressed() == 2 && Input.GetNthKeyPressed(1) == 42 && Input.GetNthKeyPressed(2) == -1\n Reads += 1\n EndIf\n\
         EndFunction\n\
         Function Fragment_2()\n\
           UnregisterForAllKeys()\n\
         EndFunction\n\
         Event OnKeyDown(Int keyCode)\n\
           If keyCode == 57\n Downs += 1\n EndIf\n\
         EndEvent\n\
         Event OnKeyUp(Int keyCode, Float holdTime)\n\
           Ups += 1\n\
           Held = holdTime\n\
         EndEvent\n\
         Event OnControlDown(String control)\n\
           If control == \"Activate\"\n Controls += 1\n EndIf\n\
         EndEvent\n\
         Event OnMenuOpen(String menuName)\n\
           Menus += 1\n\
         EndEvent\n\
         Event OnMenuClose(String menuName)\n\
           Menus += 10\n\
         EndEvent\n",
    );
    for fragment in ["Fragment_1", "Fragment_2"] {
        assert_eq!(check_function_reason(Arc::clone(&script), fragment, None).err(), None);
    }
    let mut world = world_with_fragment(script);
    world.insert_resource(crate::QuestScriptVariables::default());
    world.insert_resource(MenuRegistry::new(MenuGame::Skyrim));
    world.insert_resource(crate::fragment::HeldKeys(vec![30, 42]));
    {
        // The quest's VMAD attaches this script: SKSE delivers to every script attached to the registered object.
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        fragments.insert_vmad(
            Q,
            ScriptInstanceData {
                version: 5,
                object_format: 2,
                scripts: vec![ScriptInstance { name: "QF_VmSkseInput".into(), status: 0, properties: Vec::new() }],
            },
        );
        fragments.insert(
            Q,
            20,
            vec![Effect::Interpret { script: "QF_VmSkseInput".into(), function: "Fragment_2".into(), args: Vec::new(), resume: None }],
        );
    }
    let variable = |world: &World, name: &str| {
        world.resource::<crate::QuestScriptVariables>().get(Q, crate::QuestScriptVariables::property_key(name))
    };
    let input = |world: &World, frame: crate::fragment::InputFrame| {
        crate::fragment::deliver_input_events(world, &frame);
        crate::fragment::fragment_continuation_system(world, 0.0);
    };
    let menu = |world: &World, open: bool| {
        {
            let mut registry = world.resource_mut::<MenuRegistry>();
            if open {
                registry.open("InventoryMenu", MenuSurface::Native { page: "test".into() });
            } else {
                registry.close("InventoryMenu");
            }
        }
        crate::menu_open_close_event_system(world, 0.0);
        crate::fragment::fragment_continuation_system(world, 0.0);
    };

    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<crate::fragment::InputEventRegistrations>().entries().len(), 3);
    assert_eq!(variable(&world, "Reads"), Some(1.0), "the held keys, in code order");

    input(&world, crate::fragment::InputFrame { keys_down: vec![57, 30], controls_down: vec!["activate".into()], ..Default::default() });
    input(&world, crate::fragment::InputFrame { keys_up: vec![(57, 0.75)], ..Default::default() });
    assert_eq!((variable(&world, "Downs"), variable(&world, "Ups"), variable(&world, "Held")), (Some(1.0), Some(1.0), Some(0.75)));
    assert_eq!(variable(&world, "Controls"), Some(1.0), "the control, compared without case");
    menu(&world, true);
    menu(&world, false);
    assert_eq!(variable(&world, "Menus"), Some(11.0), "the opening and the closing");

    world.resource_mut::<QuestStageState>().set_stage(Q, 20);
    crate::fragment::quest_fragment_dispatch_system(&world);
    input(&world, crate::fragment::InputFrame { keys_down: vec![57], controls_down: vec!["Activate".into()], ..Default::default() });
    assert_eq!(variable(&world, "Downs"), Some(1.0), "the keys are unregistered");
    assert_eq!(variable(&world, "Controls"), Some(2.0), "the control stays");
}

/// LMC-SCR-004 catalog growth: SKSE's mod events. A quest script registers its object for two events and sends one
/// with no extension host: the send reaches its callback locally as `(eventName, strArg, numArg, sender)`. An event
/// built through `ModEvent` arrives on the bus with its pushed values. After `UnregisterForModEvent`, nothing
/// arrives.
#[test]
fn skse_mod_events_reach_their_callbacks() {
    use crate::script_sources::{ScriptSourceIndex, ScriptSources};
    use byroredux_sdk::event::{legacy_skse_mod_event_id, CustomEvent, LegacySkseModEventValue, LegacySkseVariadicModEventPayload};
    use byroredux_sdk::identity::PrincipalId;
    struct OneMod;
    impl ScriptSources for OneMod {
        fn source_of(&self, _: &World, _: &str) -> Option<usize> {
            Some(0)
        }
        fn scripts(&self, _: &World, _: usize) -> Vec<Vec<u8>> {
            Vec::new()
        }
        fn principal_of(&self, _: &World, _: &str) -> Option<PrincipalId> {
            PrincipalId::new("org.example.mymod").ok()
        }
    }
    let script = parsed_source(
        "ScriptName QF_VmModEvents extends Quest\n\
         Int Property Got Auto\n\
         Float Property Number Auto\n\
         Int Property Custom Auto\n\
         Function Fragment_1()\n\
           RegisterForModEvent(\"MyMod_Ready\", \"OnReady\")\n\
           RegisterForModEvent(\"MyMod_Custom\", \"OnCustom\")\n\
           SendModEvent(\"mymod_ready\", \"hello\", 2.5)\n\
         EndFunction\n\
         Function Fragment_2()\n\
           UnregisterForModEvent(\"MyMod_Ready\")\n\
           SendModEvent(\"MyMod_Ready\", \"again\")\n\
         EndFunction\n\
         Event OnReady(String eventName, String strArg, Float numArg, Form sender)\n\
           If eventName == \"mymod_ready\" && strArg == \"hello\"\n Got += 1\n EndIf\n\
           Number = numArg\n\
         EndEvent\n\
         Event OnCustom(Int count, String label)\n\
           If label == \"x\"\n Custom = count\n EndIf\n\
         EndEvent\n",
    );
    for fragment in ["Fragment_1", "Fragment_2"] {
        assert_eq!(check_function_reason(Arc::clone(&script), fragment, None).err(), None);
    }
    let mut world = world_with_fragment(script);
    world.insert_resource(crate::QuestScriptVariables::default());
    world.insert_resource(ScriptSourceIndex(Box::new(OneMod)));
    {
        let mut fragments = world.resource_mut::<crate::QuestStageFragments>();
        fragments.insert_vmad(
            Q,
            ScriptInstanceData {
                version: 5,
                object_format: 2,
                scripts: vec![ScriptInstance { name: "QF_VmModEvents".into(), status: 0, properties: Vec::new() }],
            },
        );
        fragments.insert(
            Q,
            20,
            vec![Effect::Interpret { script: "QF_VmModEvents".into(), function: "Fragment_2".into(), args: Vec::new(), resume: None }],
        );
    }
    let variable = |world: &World, name: &str| {
        world.resource::<crate::QuestScriptVariables>().get(Q, crate::QuestScriptVariables::property_key(name))
    };
    let deliver = |world: &World| {
        crate::fragment::vm_mod_event_system(world, 0.0);
        crate::fragment::fragment_continuation_system(world, 0.0);
    };

    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<crate::fragment::ModEventRegistrations>().entries().len(), 2);
    deliver(&world);
    assert_eq!(variable(&world, "Got"), Some(1.0), "the name as sent, its string");
    assert_eq!(variable(&world, "Number"), Some(2.5));

    // An event a ModEvent builder sent (another lane, a guest) arrives with its pushed values.
    let payload = LegacySkseVariadicModEventPayload {
        arguments: vec![LegacySkseModEventValue::Int(7), LegacySkseModEventValue::String("x".into())],
    };
    crate::fragment::queue_vm_mod_event(
        &world,
        CustomEvent {
            event: legacy_skse_mod_event_id("MyMod_Custom").unwrap(),
            sender: PrincipalId::new("org.example.other").unwrap(),
            payload: payload.encode().unwrap(),
        },
    );
    deliver(&world);
    assert_eq!(variable(&world, "Custom"), Some(7.0));

    world.resource_mut::<QuestStageState>().set_stage(Q, 20);
    crate::fragment::quest_fragment_dispatch_system(&world);
    deliver(&world);
    assert_eq!(variable(&world, "Got"), Some(1.0), "unregistered: the second send is not heard");
    assert_eq!(variable(&world, "Number"), Some(2.5));
}
