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
/// its top byte unused.
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
        PluginId::from_filename("Skyrim.esm"),
    )]));
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
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
        },
    );
    crate::install_owner_kinds(&mut world, &index);
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    crate::fragment::quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
    assert_eq!(world.resource::<crate::OwnerOverrides>().get(MARKER), Some(0), "the clear is kept");
    assert_eq!(world.resource::<crate::FactionCrimeGold>().of(FACTION), (45, 25));
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
            crate::ProjectileLaunch { source: MARKER, weapon: WEAPON, ammo: AMMO, aim_at: 0 },
            crate::ProjectileLaunch { source: MARKER, weapon: WEAPON, ammo: 0, aim_at: 0 },
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
