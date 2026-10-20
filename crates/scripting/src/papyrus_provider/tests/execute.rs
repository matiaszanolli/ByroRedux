//! Execution tests — these drive a live `World`.

use super::*;
use crate::ActivateEvent;

#[test]
fn self_receiver_dispatch_resolves_the_current_owner_handle() {
    let source = r#"
        ScriptName Fixture
        Event OnInit()
            self.Touch(7)
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let program = lower_provider_program(&script, &self_catalog())
        .unwrap()
        .unwrap();
    let mut world = World::new();
    crate::register(&mut world);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let calls_for_callback = Arc::clone(&calls);
    let callback = Arc::new(
        move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
            calls_for_callback
                .lock()
                .unwrap()
                .push((route.to_owned(), arguments.to_vec()));
            Ok(ScriptValue::None)
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&world, Arc::new(self_catalog()), Some(callback));
    let resolver = Arc::new(|entity: EntityId| {
        EntityRef::new(9, u64::from(entity) + 1).ok_or_else(|| "invalid test entity".to_owned())
    }) as Arc<PapyrusProviderEntityResolver>;
    set_papyrus_provider_entity_resolver(&world, Some(resolver));
    let owner = world.spawn();
    attach_papyrus_provider_program(&mut world, owner, program);
    world.insert(owner, OnInitEvent);
    papyrus_provider_system(&world, 0.0);

    assert_eq!(
        calls.lock().unwrap().as_slice(),
        [(
            "ext.org.example.self.touch-self".to_owned(),
            vec![
                ScriptValue::Entity(EntityRef::new(9, u64::from(owner) + 1).unwrap()),
                ScriptValue::Integer(7),
            ],
        )]
    );
}

#[test]
fn typed_object_receiver_dispatch_resolves_the_event_entity_handle() {
    let source = r#"
        ScriptName Fixture
        Event OnTriggerEnter(ObjectReference akActionRef)
            akActionRef.Touch(7)
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let program = lower_provider_program(&script, &object_catalog())
        .unwrap()
        .unwrap();
    let mut world = World::new();
    crate::register(&mut world);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let calls_for_callback = Arc::clone(&calls);
    let callback = Arc::new(
        move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
            calls_for_callback
                .lock()
                .unwrap()
                .push((route.to_owned(), arguments.to_vec()));
            Ok(ScriptValue::None)
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&world, Arc::new(object_catalog()), Some(callback));
    let resolver = Arc::new(|entity: EntityId| {
        EntityRef::new(9, u64::from(entity) + 1).ok_or_else(|| "invalid test entity".to_owned())
    }) as Arc<PapyrusProviderEntityResolver>;
    set_papyrus_provider_entity_resolver(&world, Some(resolver));
    let owner = world.spawn();
    let activator = world.spawn();
    attach_papyrus_provider_program(&mut world, owner, program);
    world.insert(
        owner,
        OnTriggerEnterEvent {
            triggerers: vec![activator],
        },
    );
    papyrus_provider_system(&world, 0.0);

    assert_eq!(
        calls.lock().unwrap().as_slice(),
        [(
            "ext.org.example.object.touch-object".to_owned(),
            vec![
                ScriptValue::Entity(EntityRef::new(9, u64::from(activator) + 1).unwrap()),
                ScriptValue::Integer(7),
            ],
        )]
    );
}

#[test]
fn receiver_expression_dispatch_evaluates_inner_call_before_outer_call() {
    let source = r#"
        ScriptName Fixture
        Event OnLoad()
            Game.GetPlayer().Touch(7)
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let program = lower_provider_program(&script, &object_catalog())
        .unwrap()
        .unwrap();
    let mut world = World::new();
    crate::register(&mut world);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let calls_for_callback = Arc::clone(&calls);
    let callback = Arc::new(
        move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
            calls_for_callback
                .lock()
                .unwrap()
                .push((route.to_owned(), arguments.to_vec()));
            if route == byroredux_sdk::compatibility::PAPYRUS_GAME_GET_PLAYER_ROUTE {
                Ok(ScriptValue::Entity(EntityRef::new(3, 11).unwrap()))
            } else {
                Ok(ScriptValue::None)
            }
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&world, Arc::new(object_catalog()), Some(callback));
    let entity = world.spawn();
    attach_papyrus_provider_program(&mut world, entity, program);
    world.insert(entity, OnCellLoadEvent);
    papyrus_provider_system(&world, 0.0);

    assert_eq!(
        calls.lock().unwrap().as_slice(),
        [
            (
                byroredux_sdk::compatibility::PAPYRUS_GAME_GET_PLAYER_ROUTE.to_owned(),
                Vec::new(),
            ),
            (
                "ext.org.example.object.touch-object".to_owned(),
                vec![
                    ScriptValue::Entity(EntityRef::new(3, 11).unwrap()),
                    ScriptValue::Integer(7),
                ],
            ),
        ]
    );
}

#[test]
fn receiver_expression_with_none_player_fails_closed_before_outer_call() {
    let source = r#"
        ScriptName Fixture
        Event OnLoad()
            Game.GetPlayer().Touch(7)
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let program = lower_provider_program(&script, &object_catalog())
        .unwrap()
        .unwrap();
    let mut world = World::new();
    crate::register(&mut world);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let calls_for_callback = Arc::clone(&calls);
    let callback = Arc::new(
        move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
            calls_for_callback
                .lock()
                .unwrap()
                .push((route.to_owned(), arguments.to_vec()));
            Ok(ScriptValue::None)
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&world, Arc::new(object_catalog()), Some(callback));
    let entity = world.spawn();
    attach_papyrus_provider_program(&mut world, entity, program);
    world.insert(entity, OnCellLoadEvent);
    papyrus_provider_system(&world, 0.0);

    assert_eq!(
        calls.lock().unwrap().as_slice(),
        [(
            byroredux_sdk::compatibility::PAPYRUS_GAME_GET_PLAYER_ROUTE.to_owned(),
            Vec::new(),
        )]
    );
}

#[test]
fn attached_program_dispatches_on_init_exactly_once() {
    let source = r#"
        ScriptName Fixture
        Event OnInit()
            WeatherNative.WeatherAt(4, "initialized")
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let program = lower_provider_program(&script, &catalog())
        .unwrap()
        .unwrap();
    assert_eq!(program.handler(PapyrusProviderEvent::OnInit).len(), 1);

    let mut world = World::new();
    crate::register(&mut world);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let calls_for_callback = Arc::clone(&calls);
    let callback = Arc::new(
        move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
            calls_for_callback
                .lock()
                .unwrap()
                .push((route.to_owned(), arguments.to_vec()));
            Ok(ScriptValue::String("clear".to_owned()))
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));
    let entity = world.spawn();

    attach_papyrus_provider_program(&mut world, entity, program);
    assert!(world.has::<OnInitEvent>(entity));
    papyrus_provider_system(&world, 0.0);
    crate::event_cleanup_system(&world, 0.0);
    papyrus_provider_system(&world, 0.0);

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "ext.org.example.weather.weather-at");
    assert_eq!(
        calls[0].1,
        [
            ScriptValue::Integer(4),
            ScriptValue::String("initialized".to_owned())
        ]
    );
}

/// #6017 — the program is re-attached every time its reference spawns (cell
/// re-entry, save load), but `OnInit` runs once per reference: a second entity
/// for the same FormID must not replay a non-idempotent body.
#[test]
fn provider_on_init_runs_once_per_reference_across_respawns() {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

    let source = r#"
        ScriptName Fixture
        Event OnInit()
            WeatherNative.WeatherAt(4, "initialized")
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let program = lower_provider_program(&script, &catalog()).unwrap().unwrap();

    let mut world = World::new();
    crate::register(&mut world);
    world.register::<FormIdComponent>();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let calls_for_callback = Arc::clone(&calls);
    let callback = Arc::new(
        move |_principal: Option<&PrincipalId>, route: &str, _arguments: &[ScriptValue]| {
            calls_for_callback.lock().unwrap().push(route.to_owned());
            Ok(ScriptValue::String("clear".to_owned()))
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(0x0001_2345),
    });
    world.insert_resource(pool);

    // First spawn of the reference, then it unloads and spawns again.
    for _ in 0..2 {
        let entity = world.spawn();
        world.insert(entity, FormIdComponent(id));
        attach_papyrus_provider_program(&mut world, entity, program.clone());
        papyrus_provider_system(&world, 0.0);
        crate::event_cleanup_system(&world, 0.0);
    }
    assert_eq!(calls.lock().unwrap().len(), 1, "the second spawn must not replay OnInit");
}

#[test]
fn dynamic_mod_event_registration_delivers_typed_callback_and_unregisters() {
    let source = r#"
        ScriptName Fixture
        Event OnInit()
            RegisterForModEvent("ByroReady", "OnByroReady")
        EndEvent
        Event OnByroReady(String status, Int count)
            WeatherNative.WeatherAt(count, status)
            UnregisterForModEvent("ByroReady")
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let program = lower_provider_program(&script, &catalog())
        .unwrap()
        .unwrap();
    let principal = PrincipalId::new("legacy.scripts.receiver").unwrap();

    let mut world = World::new();
    crate::register(&mut world);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&calls);
    let callback = Arc::new(
        move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
            observed
                .lock()
                .unwrap()
                .push((route.to_owned(), arguments.to_vec()));
            Ok(ScriptValue::String("clear".to_owned()))
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));
    let entity = world.spawn();
    attach_owned_papyrus_provider_program(&mut world, entity, program, principal);
    papyrus_provider_system(&world, 0.0);
    crate::event_cleanup_system(&world, 0.0);

    let sender = PrincipalId::new("legacy.scripts.sender").unwrap();
    let mut malformed = byroredux_sdk::event::LegacySkseModEventBuilders::new();
    let malformed_handle = malformed.create("ByroReady");
    malformed.push(malformed_handle, LegacySkseModEventValue::Int(7));
    let malformed = malformed.send(malformed_handle).unwrap();
    queue_papyrus_mod_event(
        &world,
        CustomEvent {
            event: malformed.event,
            sender: sender.clone(),
            payload: malformed.payload,
        },
    );
    papyrus_provider_system(&world, 0.0);
    assert!(calls.lock().unwrap().is_empty());

    let mut builders = byroredux_sdk::event::LegacySkseModEventBuilders::new();
    let handle = builders.create("ByroReady");
    builders.push(handle, LegacySkseModEventValue::String("ready".to_owned()));
    builders.push(handle, LegacySkseModEventValue::Int(7));
    let command = builders.send(handle).unwrap();
    let event = CustomEvent {
        event: command.event,
        sender,
        payload: command.payload,
    };
    queue_papyrus_mod_event(&world, event.clone());
    papyrus_provider_system(&world, 0.0);
    assert_eq!(
        *calls.lock().unwrap(),
        [(
            "ext.org.example.weather.weather-at".to_owned(),
            vec![
                ScriptValue::Integer(7),
                ScriptValue::String("ready".to_owned()),
            ],
        )]
    );

    queue_papyrus_mod_event(&world, event);
    papyrus_provider_system(&world, 0.0);
    assert_eq!(calls.lock().unwrap().len(), 1);
}

/// #5640: delivery indexes the registrations by owning entity once per
/// frame instead of walking the whole map for every program entity × every
/// pending event. Routing must be unchanged: a pending event reaches only
/// the entities registered for that event, a frame with registrations but
/// nothing pending delivers nothing, and an event nobody registered for is
/// ignored.
#[test]
fn mod_event_delivery_routes_each_event_to_its_own_registrants() {
    let script_for = |event: &str, callback: &str, tag: &str| {
        let source = format!(
            r#"
            ScriptName Fixture
            Event OnInit()
                RegisterForModEvent("{event}", "{callback}")
            EndEvent
            Event {callback}(String status, Int count)
                WeatherNative.WeatherAt(count, "{tag}")
            EndEvent
        "#
        );
        let (script, errors) = parse_script(&source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        lower_provider_program(&script, &catalog())
            .unwrap()
            .unwrap()
    };
    let mut world = World::new();
    crate::register(&mut world);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&calls);
    let callback = Arc::new(
        move |_principal: Option<&PrincipalId>, _route: &str, arguments: &[ScriptValue]| {
            observed.lock().unwrap().push(arguments.to_vec());
            Ok(ScriptValue::String("clear".to_owned()))
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));

    let entity_a = world.spawn();
    let entity_b = world.spawn();
    // Program-less bystanders: they must cost nothing and receive nothing.
    for _ in 0..50 {
        world.spawn();
    }
    attach_owned_papyrus_provider_program(
        &mut world,
        entity_a,
        script_for("EventA", "OnA", "a"),
        PrincipalId::new("legacy.scripts.a").unwrap(),
    );
    attach_owned_papyrus_provider_program(
        &mut world,
        entity_b,
        script_for("EventB", "OnB", "b"),
        PrincipalId::new("legacy.scripts.b").unwrap(),
    );
    papyrus_provider_system(&world, 0.0); // OnInit registers both
    crate::event_cleanup_system(&world, 0.0);

    let send = |name: &str, count: i32| {
        let mut builders = byroredux_sdk::event::LegacySkseModEventBuilders::new();
        let handle = builders.create(name);
        builders.push(handle, LegacySkseModEventValue::String("s".to_owned()));
        builders.push(handle, LegacySkseModEventValue::Int(count));
        let command = builders.send(handle).unwrap();
        queue_papyrus_mod_event(
            &world,
            CustomEvent {
                event: command.event,
                sender: PrincipalId::new("legacy.scripts.sender").unwrap(),
                payload: command.payload,
            },
        );
        papyrus_provider_system(&world, 0.0);
    };
    let delivered = |tag: &str, count: i32| {
        vec![ScriptValue::Integer(i64::from(count)), ScriptValue::String(tag.to_owned())]
    };

    // Registrations exist, nothing pending: nothing is delivered.
    papyrus_provider_system(&world, 0.0);
    assert!(calls.lock().unwrap().is_empty());

    send("EventA", 1);
    assert_eq!(*calls.lock().unwrap(), vec![delivered("a", 1)]);

    send("EventB", 2);
    assert_eq!(
        *calls.lock().unwrap(),
        vec![delivered("a", 1), delivered("b", 2)]
    );

    // An event with no registrants is ignored.
    send("EventC", 3);
    assert_eq!(calls.lock().unwrap().len(), 2);
}

#[test]
fn form_send_mod_event_preserves_stable_sender_across_wait() {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

    let source = r#"
        ScriptName Fixture extends Quest
        Event OnInit()
            String eventName = "ByroReady"
            Utility.Wait(1.0)
            self.SendModEvent(eventName, "ready", 7.0)
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let program = lower_provider_program(&script, &catalog())
        .unwrap()
        .unwrap();
    let principal = PrincipalId::new("legacy.scripts.sender").unwrap();
    let expected_sender = FormRef::new([9; 16], 0x1234);

    let mut world = World::new();
    crate::register(&mut world);
    let mut pool = FormIdPool::new();
    let form_id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Fixture.esm"),
        local: LocalFormId(0x1234),
    });
    world.insert_resource(pool);
    let callback = Arc::new(
        |_principal: Option<&PrincipalId>, _route: &str, _arguments: &[ScriptValue]| {
            Ok(ScriptValue::None)
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));
    set_papyrus_provider_form_resolver(&world, Some(Arc::new(move |_form_id| Ok(expected_sender))));
    let published = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&published);
    set_papyrus_provider_mod_event_publisher(
        &world,
        Some(Arc::new(move |principal, command| {
            observed.lock().unwrap().push((principal.clone(), command));
            Ok(())
        })),
    );
    let entity = world.spawn();
    world.insert(entity, FormIdComponent(form_id));
    attach_owned_papyrus_provider_program(&mut world, entity, program, principal.clone());

    papyrus_provider_system(&world, 0.0);
    crate::event_cleanup_system(&world, 0.0);
    assert!(published.lock().unwrap().is_empty());
    papyrus_provider_system(&world, 1.0);

    let published = published.lock().unwrap();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].0, principal);
    let payload =
        byroredux_sdk::event::LegacySkseModEventPayload::decode(&published[0].1.payload).unwrap();
    assert_eq!(payload.string_arg, "ready");
    assert_eq!(payload.number_arg(), 7.0);
    assert_eq!(payload.sender, Some(expected_sender));
}

#[test]
fn active_magic_effect_send_mod_event_uses_none_sender_and_defaults() {
    let source = r#"
        ScriptName Fixture extends ActiveMagicEffect
        Event OnInit()
            SendModEvent("EffectReady")
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let program = lower_provider_program(&script, &catalog())
        .unwrap()
        .unwrap();

    let mut world = World::new();
    crate::register(&mut world);
    let callback = Arc::new(
        |_principal: Option<&PrincipalId>, _route: &str, _arguments: &[ScriptValue]| {
            Ok(ScriptValue::None)
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));
    let published = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&published);
    set_papyrus_provider_mod_event_publisher(
        &world,
        Some(Arc::new(move |_principal, command| {
            observed.lock().unwrap().push(command);
            Ok(())
        })),
    );
    let entity = world.spawn();
    attach_owned_papyrus_provider_program(
        &mut world,
        entity,
        program,
        PrincipalId::new("legacy.scripts.effect").unwrap(),
    );

    papyrus_provider_system(&world, 0.0);

    let published = published.lock().unwrap();
    assert_eq!(published.len(), 1);
    let payload =
        byroredux_sdk::event::LegacySkseModEventPayload::decode(&published[0].payload).unwrap();
    assert_eq!(payload.string_arg, "");
    assert_eq!(payload.number_arg(), 0.0);
    assert_eq!(payload.sender, None);
}

#[test]
fn owned_program_preserves_its_principal_across_a_latent_tail() {
    let source = r#"
        ScriptName Fixture
        Event OnInit()
            WeatherNative.WeatherAt(1, "before")
            Utility.Wait(0.0)
            WeatherNative.WeatherAt(2, "after")
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let program = lower_provider_program(&script, &catalog())
        .unwrap()
        .unwrap();
    let principal = PrincipalId::new("legacy.scripts.fixture").unwrap();

    let mut world = World::new();
    crate::register(&mut world);
    let owners = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&owners);
    let callback = Arc::new(
        move |principal: Option<&PrincipalId>, _route: &str, _arguments: &[ScriptValue]| {
            observed
                .lock()
                .unwrap()
                .push(principal.map(ToString::to_string));
            Ok(ScriptValue::String("ok".to_owned()))
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));
    let entity = world.spawn();
    attach_owned_papyrus_provider_program(&mut world, entity, program, principal.clone());

    papyrus_provider_system(&world, 0.0);
    assert_eq!(
        world.resource::<PapyrusProviderContinuationQueue>().len(),
        1
    );
    crate::event_cleanup_system(&world, 0.0);
    papyrus_provider_system(&world, 0.0);

    assert_eq!(
        *owners.lock().unwrap(),
        [Some(principal.to_string()), Some(principal.to_string())]
    );
}

#[test]
fn multiple_attached_scripts_preserve_handler_order_without_overwrite() {
    let first_source = r#"
        ScriptName FirstFixture
        Event OnInit()
            WeatherNative.WeatherAt(1, "first-init")
        EndEvent
        Event OnLoad()
            WeatherNative.WeatherAt(2, "first-load")
        EndEvent
    "#;
    let second_source = r#"
        ScriptName SecondFixture
        Event OnInit()
            WeatherNative.WeatherAt(3, "second-init")
        EndEvent
        Event OnLoad()
            WeatherNative.WeatherAt(4, "second-load")
        EndEvent
        Event OnActivate()
            WeatherNative.WeatherAt(5, "second-activate")
        EndEvent
    "#;
    let lower = |source| {
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        lower_provider_program(&script, &catalog())
            .unwrap()
            .unwrap()
    };

    let mut world = World::new();
    crate::register(&mut world);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&calls);
    let callback = Arc::new(
        move |_principal: Option<&PrincipalId>, _route: &str, arguments: &[ScriptValue]| {
            observed.lock().unwrap().push(arguments.to_vec());
            Ok(ScriptValue::String("ok".to_owned()))
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));
    let entity = world.spawn();
    attach_papyrus_provider_program(&mut world, entity, lower(first_source));
    attach_papyrus_provider_program(&mut world, entity, lower(second_source));
    assert_eq!(
        world
            .get::<PapyrusProviderProgram>(entity)
            .unwrap()
            .handlers_for(PapyrusProviderEvent::OnLoad)
            .count(),
        2
    );
    world.insert(entity, OnCellLoadEvent);
    world.insert(entity, ActivateEvent { activator: entity });

    papyrus_provider_system(&world, 0.0);

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 5);
    assert_eq!(
        calls
            .iter()
            .map(|arguments| arguments[0].clone())
            .collect::<Vec<_>>(),
        [1, 3, 2, 4, 5]
            .into_iter()
            .map(ScriptValue::Integer)
            .collect::<Vec<_>>()
    );
}

#[test]
fn combat_and_equipment_events_dispatch_in_batch_order() {
    let source = r#"
        ScriptName Fixture
        Event OnHit(ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked)
            If abPowerAttack && !abHitBlocked
                WeatherNative.WeatherAt(1, "hit")
            EndIf
        EndEvent
        Event OnObjectEquipped(Form akBaseObject, ObjectReference akReference)
            WeatherNative.InspectForm(akBaseObject)
        EndEvent
        Event OnObjectUnequipped(Form akBaseObject, ObjectReference akReference)
            WeatherNative.InspectForm(akBaseObject)
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let program = lower_provider_program(&script, &catalog())
        .unwrap()
        .unwrap();

    let mut world = World::new();
    crate::register(&mut world);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let calls_for_callback = Arc::clone(&calls);
    let callback = Arc::new(
        move |_principal: Option<&PrincipalId>, _route: &str, arguments: &[ScriptValue]| {
            calls_for_callback.lock().unwrap().push(arguments.to_vec());
            Ok(ScriptValue::String("clear".to_owned()))
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));
    let form_resolver = Arc::new(|form_id: u32| Ok(FormRef::new([7; 16], form_id)))
        as Arc<PapyrusProviderFormResolver>;
    set_papyrus_provider_form_resolver(&world, Some(form_resolver));
    let entity = world.spawn();
    let aggressor = world.spawn();
    attach_papyrus_provider_program(&mut world, entity, program);
    world.insert(
        entity,
        HitEvent {
            aggressor,
            source: aggressor,
            projectile: 0,
            damage: 10.0,
            power_attack: true,
            sneak_attack: false,
            bash_attack: false,
            blocked: false,
        },
    );
    world.insert(
        entity,
        EquipmentEventBatch(vec![
            crate::EquipmentChange {
                item_form_id: 1,
                equipped: false,
            },
            crate::EquipmentChange {
                item_form_id: 2,
                equipped: true,
            },
        ]),
    );

    papyrus_provider_system(&world, 0.0);

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0][0], ScriptValue::Integer(1));
    assert_eq!(calls[1], [ScriptValue::Form(FormRef::new([7; 16], 1))]);
    assert_eq!(calls[2], [ScriptValue::Form(FormRef::new([7; 16], 2))]);
}

#[test]
fn equipment_form_identity_survives_a_latent_handler_tail() {
    let source = r#"
        ScriptName Fixture
        Event OnObjectEquipped(Form akBaseObject, ObjectReference akReference)
            Utility.Wait(0.5)
            WeatherNative.InspectForm(akBaseObject)
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let program = lower_provider_program(&script, &catalog())
        .unwrap()
        .unwrap();

    let mut world = World::new();
    crate::register(&mut world);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let calls_for_callback = Arc::clone(&calls);
    let callback = Arc::new(
        move |_principal: Option<&PrincipalId>, _route: &str, arguments: &[ScriptValue]| {
            calls_for_callback.lock().unwrap().push(arguments.to_vec());
            Ok(ScriptValue::String("ok".to_owned()))
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));
    let form_resolver = Arc::new(|form_id: u32| Ok(FormRef::new([8; 16], form_id)))
        as Arc<PapyrusProviderFormResolver>;
    set_papyrus_provider_form_resolver(&world, Some(form_resolver));
    let entity = world.spawn();
    attach_papyrus_provider_program(&mut world, entity, program);
    world.insert(
        entity,
        EquipmentEventBatch(vec![crate::EquipmentChange {
            item_form_id: 0x44,
            equipped: true,
        }]),
    );

    papyrus_provider_system(&world, 0.0);
    assert!(calls.lock().unwrap().is_empty());
    assert_eq!(
        world.resource::<PapyrusProviderContinuationQueue>().len(),
        1
    );
    crate::event_cleanup_system(&world, 0.0);

    papyrus_provider_system(&world, 0.5);

    assert_eq!(
        *calls.lock().unwrap(),
        vec![vec![ScriptValue::Form(FormRef::new([8; 16], 0x44))]]
    );
    assert!(world
        .resource::<PapyrusProviderContinuationQueue>()
        .is_empty());
}

#[test]
fn source_handler_assigns_a_typed_result_and_selects_one_branch() {
    let source = r#"
        ScriptName Fixture
        Event OnLoad()
            Bool storm
            storm = WeatherNative.IsStorm()
            If storm
                WeatherNative.WeatherAt(4, "clear")
            Else
                WeatherNative.WeatherAt(5, "cloudy")
            EndIf
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let program = lower_provider_program(&script, &catalog())
        .unwrap()
        .unwrap();

    let mut world = World::new();
    crate::register(&mut world);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let calls_for_callback = Arc::clone(&calls);
    let callback = Arc::new(
        move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
            calls_for_callback
                .lock()
                .unwrap()
                .push((route.to_owned(), arguments.to_vec()));
            if route.ends_with("is-storm") {
                Ok(ScriptValue::Boolean(true))
            } else {
                Ok(ScriptValue::String("rain".to_owned()))
            }
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));
    let entity = world.spawn();
    attach_papyrus_provider_program(&mut world, entity, program);
    world.insert(entity, OnCellLoadEvent);

    papyrus_provider_system(&world, 0.0);

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].0, "ext.org.example.weather.is-storm");
    assert_eq!(calls[1].0, "ext.org.example.weather.weather-at");
    assert_eq!(
        calls[1].1,
        [
            ScriptValue::Integer(4),
            ScriptValue::String("clear".to_owned())
        ]
    );
}

#[test]
fn latent_wait_preserves_locals_and_branch_and_handler_tails() {
    let source = r#"
        ScriptName Fixture
        Event OnLoad()
            Bool storm
            String branchLabel = "after-branch-wait"
            storm = WeatherNative.IsStorm()
            If storm
                Utility.Wait(0.5)
                WeatherNative.WeatherAt(4, branchLabel)
            EndIf
            WeatherNative.WeatherAt(5, "handler-tail")
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let provider_catalog = catalog();
    let program = lower_provider_program(&script, &provider_catalog)
        .unwrap()
        .unwrap();

    let mut world = World::new();
    crate::register(&mut world);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let calls_for_callback = Arc::clone(&calls);
    let callback = Arc::new(
        move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
            calls_for_callback
                .lock()
                .unwrap()
                .push((route.to_owned(), arguments.to_vec()));
            if route.ends_with("is-storm") {
                Ok(ScriptValue::Boolean(true))
            } else {
                Ok(ScriptValue::String("ok".to_owned()))
            }
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&world, Arc::new(provider_catalog), Some(callback));
    let entity = world.spawn();
    attach_papyrus_provider_program(&mut world, entity, program);
    world.insert(entity, OnCellLoadEvent);

    papyrus_provider_system(&world, 0.0);
    assert_eq!(calls.lock().unwrap().len(), 1);
    assert_eq!(
        world.resource::<PapyrusProviderContinuationQueue>().len(),
        1
    );
    world.query_mut::<OnCellLoadEvent>().unwrap().remove(entity);

    papyrus_provider_system(&world, 0.25);
    assert_eq!(calls.lock().unwrap().len(), 1);
    assert_eq!(
        world.resource::<PapyrusProviderContinuationQueue>().len(),
        1
    );

    papyrus_provider_system(&world, 0.25);
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[1].1[0], ScriptValue::Integer(4));
    assert_eq!(
        calls[1].1[1],
        ScriptValue::String("after-branch-wait".to_owned())
    );
    assert_eq!(calls[2].1[0], ScriptValue::Integer(5));
    assert!(world
        .resource::<PapyrusProviderContinuationQueue>()
        .is_empty());
}

#[test]
fn restored_continuation_rejects_a_route_not_in_the_live_catalog() {
    let source = r#"
        ScriptName Fixture
        Event OnLoad()
            Game.GetModCount()
            Utility.Wait(0.0)
            Game.IsPluginInstalled("Update.esm")
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let provider_catalog = PapyrusProviderCatalog::engine_compatibility();
    let program = lower_provider_program(&script, &provider_catalog)
        .unwrap()
        .unwrap();

    let mut world = World::new();
    crate::register(&mut world);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let calls_for_callback = Arc::clone(&calls);
    let callback = Arc::new(
        move |_principal: Option<&PrincipalId>, route: &str, _arguments: &[ScriptValue]| {
            calls_for_callback.lock().unwrap().push(route.to_owned());
            Ok(ScriptValue::None)
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&world, Arc::new(provider_catalog), Some(callback));
    let entity = world.spawn();
    attach_papyrus_provider_program(&mut world, entity, program);
    world.insert(entity, OnCellLoadEvent);

    papyrus_provider_system(&world, 0.0);
    world.query_mut::<OnCellLoadEvent>().unwrap().remove(entity);
    {
        let mut queue = world.resource_mut::<PapyrusProviderContinuationQueue>();
        let PapyrusProviderStatement::Call(call) = &mut queue.pending[0].statements[0] else {
            panic!("expected saved provider call tail");
        };
        call.route.qualified_name = "ext.attacker.privileged".to_owned();
    }

    papyrus_provider_system(&world, 0.0);

    assert_eq!(calls.lock().unwrap().len(), 1);
    assert!(world
        .resource::<PapyrusProviderContinuationQueue>()
        .is_empty());
}

#[test]
fn provider_results_support_comparisons_and_short_circuit_conditions() {
    let source = r#"
        ScriptName Fixture
        Event OnLoad()
            Int count
            count = Game.GetModCount()
            If count >= 2 && !Game.IsPluginInstalled("Missing.esp")
                WeatherNative.WeatherAt(4, "matched")
            Else
                WeatherNative.WeatherAt(5, "missed")
            EndIf
            If true || Game.IsPluginInstalled("MustNotRun.esp")
                WeatherNative.WeatherAt(6, "short-circuited")
            EndIf
            String weather
            weather = WeatherNative.WeatherAt(0, "probe")
            If weather == "rain"
                WeatherNative.WeatherAt(7, weather)
            EndIf
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let mut provider_catalog = PapyrusProviderCatalog::engine_compatibility();
    let extension = ExtensionId::new("org.example.weather").unwrap();
    provider_catalog.insert(&extension, &declaration()).unwrap();
    provider_catalog
        .insert(&extension, &boolean_declaration())
        .unwrap();
    let program = lower_provider_program(&script, &provider_catalog)
        .unwrap()
        .unwrap();

    let mut world = World::new();
    crate::register(&mut world);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let calls_for_callback = Arc::clone(&calls);
    let callback = Arc::new(
        move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
            calls_for_callback
                .lock()
                .unwrap()
                .push((route.to_owned(), arguments.to_vec()));
            if route.ends_with("get-mod-count") {
                Ok(ScriptValue::Integer(2))
            } else if route.ends_with("is-plugin-installed") {
                Ok(ScriptValue::Boolean(false))
            } else if arguments.first() == Some(&ScriptValue::Integer(0)) {
                Ok(ScriptValue::String("rain".to_owned()))
            } else {
                Ok(ScriptValue::String("ok".to_owned()))
            }
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&world, Arc::new(provider_catalog), Some(callback));
    let entity = world.spawn();
    attach_papyrus_provider_program(&mut world, entity, program);
    world.insert(entity, OnCellLoadEvent);

    papyrus_provider_system(&world, 0.0);

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 6);
    assert!(calls[0].0.ends_with("get-mod-count"));
    assert!(calls[1].0.ends_with("is-plugin-installed"));
    assert_eq!(calls[2].1[0], ScriptValue::Integer(4));
    assert_eq!(calls[3].1[0], ScriptValue::Integer(6));
    assert_eq!(calls[4].1[0], ScriptValue::Integer(0));
    assert_eq!(calls[5].1[0], ScriptValue::Integer(7));
    assert_eq!(calls[5].1[1], ScriptValue::String("rain".to_owned()));
    assert!(calls.iter().all(|(_, arguments)| arguments.first()
        != Some(&ScriptValue::String("MustNotRun.esp".to_owned()))));
}

#[test]
fn provider_expressions_execute_typed_arithmetic_and_string_concatenation() {
    let source = r#"
        ScriptName Fixture
        Event OnLoad()
            Int count
            count = Game.GetModCount() + 2
            String label
            label = "prefix-" + WeatherNative.WeatherAt(count, "fallback")
            If count * 2 >= 10
                WeatherNative.WeatherAt(count, label)
            EndIf
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let mut provider_catalog = PapyrusProviderCatalog::engine_compatibility();
    let extension = ExtensionId::new("org.example.weather").unwrap();
    provider_catalog.insert(&extension, &declaration()).unwrap();
    let program = lower_provider_program(&script, &provider_catalog)
        .unwrap()
        .unwrap();
    assert!(program
        .handler(PapyrusProviderEvent::OnLoad)
        .iter()
        .any(|statement| matches!(
            statement,
            PapyrusProviderStatement::AssignValue {
                value: PapyrusProviderValue::Binary {
                    operator: PapyrusProviderArithmetic::StrCat,
                    ..
                },
                value_type: ScriptValueType::String,
                ..
            }
        )));

    let mut world = World::new();
    crate::register(&mut world);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&calls);
    let callback = Arc::new(
        move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
            observed
                .lock()
                .unwrap()
                .push((route.to_owned(), arguments.to_vec()));
            if route.ends_with("get-mod-count") {
                Ok(ScriptValue::Integer(3))
            } else {
                Ok(ScriptValue::String("rain".to_owned()))
            }
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&world, Arc::new(provider_catalog), Some(callback));
    let entity = world.spawn();
    attach_papyrus_provider_program(&mut world, entity, program);
    world.insert(entity, OnCellLoadEvent);

    papyrus_provider_system(&world, 0.0);

    assert_eq!(
        *calls.lock().unwrap(),
        vec![
            (
                byroredux_sdk::compatibility::PAPYRUS_GAME_GET_MOD_COUNT_ROUTE.to_owned(),
                Vec::new(),
            ),
            (
                "ext.org.example.weather.weather-at".to_owned(),
                vec![
                    ScriptValue::Integer(5),
                    ScriptValue::String("fallback".to_owned()),
                ],
            ),
            (
                "ext.org.example.weather.weather-at".to_owned(),
                vec![
                    ScriptValue::Integer(5),
                    ScriptValue::String("prefix-rain".to_owned()),
                ],
            ),
        ]
    );
}

#[test]
fn game_get_player_binds_to_an_opaque_object_local() {
    let source = r#"
        ScriptName Fixture
        Event OnLoad()
            ObjectReference player
            player = Game.GetPlayer()
            WeatherNative.InspectEntity(player)
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let mut provider_catalog = PapyrusProviderCatalog::engine_compatibility();
    let extension = ExtensionId::new("org.example.weather").unwrap();
    provider_catalog
        .insert(&extension, &entity_declaration())
        .unwrap();
    let program = lower_provider_program(&script, &provider_catalog)
        .unwrap()
        .unwrap();

    let mut world = World::new();
    crate::register(&mut world);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&calls);
    let callback = Arc::new(
        move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
            observed
                .lock()
                .unwrap()
                .push((route.to_owned(), arguments.to_vec()));
            if route == byroredux_sdk::compatibility::PAPYRUS_GAME_GET_PLAYER_ROUTE {
                Ok(ScriptValue::Entity(EntityRef::new(1, 7).unwrap()))
            } else {
                Ok(ScriptValue::String("inspected".to_owned()))
            }
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&world, Arc::new(provider_catalog), Some(callback));
    let entity = world.spawn();
    attach_papyrus_provider_program(&mut world, entity, program);
    world.insert(entity, OnCellLoadEvent);

    papyrus_provider_system(&world, 0.0);

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[0],
        (
            byroredux_sdk::compatibility::PAPYRUS_GAME_GET_PLAYER_ROUTE.to_owned(),
            Vec::new()
        )
    );
    assert_eq!(calls[1].0, "ext.org.example.weather.inspect-entity");
    assert_eq!(
        calls[1].1,
        vec![ScriptValue::Entity(EntityRef::new(1, 7).unwrap())]
    );
}

#[test]
fn entity_conditions_support_identity_and_nullable_none() {
    let source = r#"
        ScriptName Fixture
        Event OnLoad()
            ObjectReference player
            player = Game.GetPlayer()
            If player == player
                WeatherNative.WeatherAt(1, "same")
            EndIf
            If player != None
                WeatherNative.WeatherAt(2, "present")
            EndIf
            If player == None
                WeatherNative.WeatherAt(3, "unexpected")
            EndIf
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let mut provider_catalog = PapyrusProviderCatalog::engine_compatibility();
    let extension = ExtensionId::new("org.example.weather").unwrap();
    provider_catalog.insert(&extension, &declaration()).unwrap();
    let program = lower_provider_program(&script, &provider_catalog)
        .unwrap()
        .unwrap();

    let mut world = World::new();
    crate::register(&mut world);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&calls);
    let callback = Arc::new(
        move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
            observed
                .lock()
                .unwrap()
                .push((route.to_owned(), arguments.to_vec()));
            if route == byroredux_sdk::compatibility::PAPYRUS_GAME_GET_PLAYER_ROUTE {
                Ok(ScriptValue::Entity(EntityRef::new(1, 7).unwrap()))
            } else {
                Ok(ScriptValue::String("ok".to_owned()))
            }
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&world, Arc::new(provider_catalog), Some(callback));
    let entity = world.spawn();
    attach_papyrus_provider_program(&mut world, entity, program);
    world.insert(entity, OnCellLoadEvent);

    papyrus_provider_system(&world, 0.0);

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 3);
    assert_eq!(
        calls[0],
        (
            byroredux_sdk::compatibility::PAPYRUS_GAME_GET_PLAYER_ROUTE.to_owned(),
            Vec::new()
        )
    );
    assert_eq!(calls[1].0, "ext.org.example.weather.weather-at");
    assert_eq!(calls[1].1[0], ScriptValue::Integer(1));
    assert_eq!(calls[2].0, "ext.org.example.weather.weather-at");
    assert_eq!(calls[2].1[0], ScriptValue::Integer(2));
}

#[test]
fn entity_conditions_match_none_when_engine_player_is_missing() {
    let source = r#"
        ScriptName Fixture
        Event OnLoad()
            ObjectReference player
            player = Game.GetPlayer()
            If player == None
                WeatherNative.WeatherAt(3, "none")
            EndIf
            If player != None
                WeatherNative.WeatherAt(4, "unexpected")
            EndIf
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let mut provider_catalog = PapyrusProviderCatalog::engine_compatibility();
    let extension = ExtensionId::new("org.example.weather").unwrap();
    provider_catalog.insert(&extension, &declaration()).unwrap();
    let program = lower_provider_program(&script, &provider_catalog)
        .unwrap()
        .unwrap();

    let mut world = World::new();
    crate::register(&mut world);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&calls);
    let callback = Arc::new(
        move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
            observed
                .lock()
                .unwrap()
                .push((route.to_owned(), arguments.to_vec()));
            if route == byroredux_sdk::compatibility::PAPYRUS_GAME_GET_PLAYER_ROUTE {
                Ok(ScriptValue::None)
            } else {
                Ok(ScriptValue::String("ok".to_owned()))
            }
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&world, Arc::new(provider_catalog), Some(callback));
    let entity = world.spawn();
    attach_papyrus_provider_program(&mut world, entity, program);
    world.insert(entity, OnCellLoadEvent);

    papyrus_provider_system(&world, 0.0);

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[0],
        (
            byroredux_sdk::compatibility::PAPYRUS_GAME_GET_PLAYER_ROUTE.to_owned(),
            Vec::new()
        )
    );
    assert_eq!(calls[1].0, "ext.org.example.weather.weather-at");
    assert_eq!(calls[1].1[0], ScriptValue::Integer(3));
}

#[test]
fn trigger_enter_and_update_events_dispatch_provider_handlers() {
    let source = r#"
        ScriptName Fixture
        Event OnTriggerEnter(ObjectReference akActionRef)
            WeatherNative.InspectEntity(akActionRef)
        EndEvent
        Event OnUpdate()
            WeatherNative.WeatherAt(8, "update")
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let program = lower_provider_program(&script, &catalog())
        .unwrap()
        .unwrap();

    let mut world = World::new();
    crate::register(&mut world);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let calls_for_callback = Arc::clone(&calls);
    let callback = Arc::new(
        move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
            calls_for_callback
                .lock()
                .unwrap()
                .push((route.to_owned(), arguments.to_vec()));
            Ok(ScriptValue::String("ok".to_owned()))
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));
    let resolver = Arc::new(|entity: EntityId| {
        EntityRef::new(9, u64::from(entity) + 1).ok_or_else(|| "invalid test entity".to_owned())
    }) as Arc<PapyrusProviderEntityResolver>;
    set_papyrus_provider_entity_resolver(&world, Some(resolver));
    let entity = world.spawn();
    let first_triggerer = world.spawn();
    let second_triggerer = world.spawn();
    attach_papyrus_provider_program(&mut world, entity, program);
    world.insert(
        entity,
        OnTriggerEnterEvent {
            triggerers: vec![first_triggerer, second_triggerer],
        },
    );
    world.insert(entity, OnUpdateEvent);

    papyrus_provider_system(&world, 0.0);

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 3);
    assert_eq!(
        calls[0].1[0],
        ScriptValue::Entity(EntityRef::new(9, u64::from(first_triggerer) + 1).unwrap())
    );
    assert_eq!(
        calls[1].1[0],
        ScriptValue::Entity(EntityRef::new(9, u64::from(second_triggerer) + 1).unwrap())
    );
    assert_eq!(calls[2].1[0], ScriptValue::Integer(8));
}

// ── execute_statements: explicit cursor stack (#5635) ─────────────────────

/// (outcome, event names) — what [`run_statements`] reports back.
type RunStatementsOutcome = (
    Result<Option<(f32, Vec<PapyrusProviderStatement>)>, String>,
    Vec<String>,
);

/// Drive `execute_statements` directly and report the event names it
/// registered (the ordered, observable side effect a `RegisterModEvent`
/// leaves) plus its outcome.
fn run_statements(statements: &[PapyrusProviderStatement]) -> RunStatementsOutcome {
    let callback = Arc::new(
        |_principal: Option<&PrincipalId>, _route: &str, _arguments: &[ScriptValue]| {
            Ok(ScriptValue::None)
        },
    ) as Arc<PapyrusProviderCallback>;
    let mut locals = BTreeMap::new();
    let mut registrations = Vec::new();
    let outcome = super::super::execute::execute_statements(
        statements,
        callback.as_ref(),
        None,
        None,
        &mut locals,
        &mut registrations,
    );
    let names = registrations
        .into_iter()
        .map(|action| match action {
            PapyrusModEventRegistrationAction::Register { event_name, .. } => event_name,
            other => panic!("unexpected registration action {other:?}"),
        })
        .collect();
    (outcome, names)
}

fn register(name: &str) -> PapyrusProviderStatement {
    PapyrusProviderStatement::RegisterModEvent {
        event_name: name.to_owned(),
        callback: "cb".to_owned(),
    }
}

fn registered(name: &str) -> String {
    name.to_owned()
}

fn if_literal(
    condition: bool,
    then_branch: Vec<PapyrusProviderStatement>,
    else_branch: Vec<PapyrusProviderStatement>,
) -> PapyrusProviderStatement {
    PapyrusProviderStatement::If {
        condition: Box::new(PapyrusProviderCondition::Literal(condition)),
        then_branch,
        else_branch,
    }
}

/// N sequential `If`s used to cost N stack frames and O(N^2) live statement
/// clones (`branch ++ tail` rebuilt and recursed into per executed `If`) —
/// 20 000 of them on a small stack overflowed or ran out of memory long
/// before finishing. They must now run in linear time on O(1) stack, in
/// authored order.
#[test]
fn sequential_ifs_execute_without_recursion_or_tail_clones() {
    const N: usize = 20_000;
    let handle = std::thread::Builder::new()
        // Far below what 20 000 recursive frames need.
        .stack_size(256 * 1024)
        .spawn(|| {
            let statements: Vec<_> = (0..N)
                .map(|i| if_literal(true, vec![register(&format!("e{i}"))], Vec::new()))
                .collect();
            let (outcome, registrations) = run_statements(&statements);
            assert_eq!(outcome, Ok(None));
            assert_eq!(registrations.len(), N);
            assert_eq!(registrations[0], registered("e0"));
            assert_eq!(registrations[N - 1], registered(&format!("e{}", N - 1)));
        })
        .unwrap();
    handle.join().expect("sequential Ifs executed on a small stack");
}

/// A `Wait` nested two `If`s deep suspends with everything still pending —
/// the rest of the innermost branch, then of each enclosing branch, then
/// the handler tail — in execution order, and the untaken `else` never runs.
#[test]
fn wait_inside_nested_ifs_suspends_with_the_remaining_program_in_order() {
    let statements = vec![
        register("a"),
        if_literal(
            true,
            vec![
                register("b"),
                if_literal(
                    true,
                    vec![
                        register("c"),
                        PapyrusProviderStatement::Wait { seconds: 1.5 },
                        register("d"),
                    ],
                    vec![register("never-inner")],
                ),
                register("e"),
            ],
            vec![register("never-outer")],
        ),
        register("f"),
    ];
    let (outcome, registrations) = run_statements(&statements);
    assert_eq!(
        registrations,
        vec![registered("a"), registered("b"), registered("c")]
    );
    assert_eq!(
        outcome,
        Ok(Some((1.5, vec![register("d"), register("e"), register("f")])))
    );

    // The same handler resumed from the suspended tail finishes in order.
    let (_, tail) = outcome.unwrap().unwrap();
    let (resumed, resumed_registrations) = run_statements(&tail);
    assert_eq!(resumed, Ok(None));
    assert_eq!(
        resumed_registrations,
        vec![registered("d"), registered("e"), registered("f")]
    );
}

/// The false arm runs instead, and an empty chosen branch falls through to
/// the rest of the slice.
#[test]
fn if_takes_the_selected_branch_and_falls_through_when_it_is_empty() {
    let statements = vec![
        if_literal(false, vec![register("then")], vec![register("else")]),
        if_literal(true, Vec::new(), vec![register("unreached")]),
        register("after"),
    ];
    let (outcome, registrations) = run_statements(&statements);
    assert_eq!(outcome, Ok(None));
    assert_eq!(registrations, vec![registered("else"), registered("after")]);
}

fn mod_event(name: &str) -> CustomEvent {
    let mut builders = byroredux_sdk::event::LegacySkseModEventBuilders::new();
    let handle = builders.create(name);
    builders.push(handle, LegacySkseModEventValue::String("s".to_owned()));
    builders.push(handle, LegacySkseModEventValue::Int(1));
    let command = builders.send(handle).unwrap();
    CustomEvent {
        event: command.event,
        sender: PrincipalId::new("legacy.scripts.sender").unwrap(),
        payload: command.payload,
    }
}

fn registration_key(
    entity: EntityId,
    principal: &PrincipalId,
    event: &CustomEvent,
) -> (EntityId, PrincipalId, byroredux_sdk::identity::EventId) {
    (entity, principal.clone(), event.event.clone())
}

fn registering_program(event: &str) -> PapyrusProviderProgram {
    let source = format!(
        r#"
        ScriptName Fixture
        Event OnInit()
            RegisterForModEvent("{event}", "OnReady")
        EndEvent
        Event OnReady(String status, Int count)
            WeatherNative.WeatherAt(count, status)
        EndEvent
    "#
    );
    let (script, errors) = parse_script(&source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    lower_provider_program(&script, &catalog())
        .unwrap()
        .unwrap()
}

/// #6023: a handler's `RegisterForModEvent` runs before a later statement
/// fails; the earlier provider calls already had effect, so the registration
/// is kept rather than discarded with the error.
#[test]
fn a_failing_provider_call_keeps_the_registrations_collected_before_it() {
    let source = r#"
        ScriptName Fixture
        Event OnInit()
            RegisterForModEvent("ByroReady", "OnByroReady")
            WeatherNative.WeatherAt(1, "x")
        EndEvent
        Event OnByroReady(String status, Int count)
            WeatherNative.WeatherAt(count, status)
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let program = lower_provider_program(&script, &catalog())
        .unwrap()
        .unwrap();
    let mut world = World::new();
    crate::register(&mut world);
    let callback = Arc::new(
        |_principal: Option<&PrincipalId>, _route: &str, _arguments: &[ScriptValue]| {
            Err("transient host failure".to_owned())
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));
    let entity = world.spawn();
    attach_owned_papyrus_provider_program(
        &mut world,
        entity,
        program,
        PrincipalId::new("legacy.scripts.receiver").unwrap(),
    );
    papyrus_provider_system(&world, 0.0);

    let runtime = world.resource::<PapyrusModEventRuntime>();
    assert_eq!(
        runtime.registrations.len(),
        1,
        "the registration made before the failing call must survive"
    );
}

/// #6018: a despawned registrant can never match again, yet its row used to
/// stay forever. Pending delivery drops it, and a full table frees dead rows
/// for a live registration instead of refusing it for the rest of the session.
#[test]
fn registrations_of_despawned_registrants_are_pruned() {
    let mut world = World::new();
    crate::register(&mut world);
    set_papyrus_provider_runtime(
        &world,
        Arc::new(catalog()),
        Some(Arc::new(|_: Option<&PrincipalId>, _: &str, _: &[ScriptValue]| Ok(ScriptValue::None))
            as Arc<PapyrusProviderCallback>),
    );
    let principal = PrincipalId::new("legacy.scripts.receiver").unwrap();
    let ready = mod_event("ByroReady");

    // Delivery path: register, despawn the registrant, then deliver.
    let doomed = world.spawn();
    attach_owned_papyrus_provider_program(
        &mut world,
        doomed,
        registering_program("ByroReady"),
        principal.clone(),
    );
    papyrus_provider_system(&world, 0.0);
    crate::event_cleanup_system(&world, 0.0);
    assert_eq!(world.resource::<PapyrusModEventRuntime>().registrations.len(), 1);
    world.despawn(doomed);
    queue_papyrus_mod_event(&world, ready.clone());
    papyrus_provider_system(&world, 0.0);
    assert!(
        world.resource::<PapyrusModEventRuntime>().registrations.is_empty(),
        "the despawned registrant's row is dropped when an event is delivered"
    );

    // Cap path: a table full of dead rows must not refuse a live registration.
    let dead: Vec<EntityId> = (0..MAX_PAPYRUS_MOD_EVENT_REGISTRATIONS)
        .map(|_| world.spawn())
        .collect();
    {
        let mut runtime = world.resource_mut::<PapyrusModEventRuntime>();
        for dead in dead {
            runtime
                .registrations
                .insert(registration_key(dead, &principal, &ready), "OnReady".to_owned());
        }
        assert_eq!(runtime.registrations.len(), MAX_PAPYRUS_MOD_EVENT_REGISTRATIONS);
    }
    let live = world.spawn();
    attach_owned_papyrus_provider_program(
        &mut world,
        live,
        registering_program("ByroReady"),
        principal.clone(),
    );
    papyrus_provider_system(&world, 0.0);
    let runtime = world.resource::<PapyrusModEventRuntime>();
    assert_eq!(runtime.registrations.len(), 1, "dead rows freed, live one kept");
    assert!(runtime
        .registrations
        .contains_key(&registration_key(live, &principal, &ready)));
}

/// #6019: pending events are delivered up to a per-frame registration budget;
/// the rest stay queued in order rather than all being materialised at once.
#[test]
fn mod_event_delivery_is_budgeted_per_frame_and_carries_the_rest() {
    let principal = PrincipalId::new("legacy.scripts.receiver").unwrap();
    let big = mod_event("Big");
    let small = mod_event("Small");
    let ignored = mod_event("Nobody");
    let mut registrations = BTreeMap::new();
    for entity in 0..(MAX_PAPYRUS_MOD_EVENT_DELIVERIES_PER_FRAME as u32 + 10) {
        registrations.insert(registration_key(entity, &principal, &big), "OnBig".to_owned());
    }
    registrations.insert(registration_key(0, &principal, &small), "OnSmall".to_owned());

    // One event is always taken whole, even over budget; the next waits.
    let pending = [big.clone(), small.clone()];
    assert_eq!(mod_event_batch_len(&pending, &registrations), 1);
    // Events nobody registered for are free and ride along; an over-budget
    // event behind them waits for a frame of its own.
    let pending = [ignored.clone(), big.clone(), ignored.clone(), small.clone()];
    assert_eq!(mod_event_batch_len(&pending, &registrations), 1);
    let pending = [small.clone(), ignored.clone(), small.clone(), big.clone()];
    assert_eq!(mod_event_batch_len(&pending, &registrations), 3);
    // Cheap events share a frame.
    let pending = [small.clone(), small.clone(), ignored.clone()];
    assert_eq!(mod_event_batch_len(&pending, &registrations), 3);

    // Through the system: the deferred event is still queued after one frame
    // and is taken on the next. Real registrants (the prune keeps live rows).
    let mut world = World::new();
    crate::register(&mut world);
    set_papyrus_provider_runtime(
        &world,
        Arc::new(catalog()),
        Some(Arc::new(|_: Option<&PrincipalId>, _: &str, _: &[ScriptValue]| Ok(ScriptValue::None))
            as Arc<PapyrusProviderCallback>),
    );
    let big_program = registering_program("Big");
    for _ in 0..(MAX_PAPYRUS_MOD_EVENT_DELIVERIES_PER_FRAME + 10) {
        let entity = world.spawn();
        attach_owned_papyrus_provider_program(&mut world, entity, big_program.clone(), principal.clone());
    }
    let small_entity = world.spawn();
    attach_owned_papyrus_provider_program(
        &mut world,
        small_entity,
        registering_program("Small"),
        principal.clone(),
    );
    papyrus_provider_system(&world, 0.0); // OnInit registers everyone
    crate::event_cleanup_system(&world, 0.0);
    queue_papyrus_mod_event(&world, big);
    queue_papyrus_mod_event(&world, small.clone());
    papyrus_provider_system(&world, 0.0);
    assert_eq!(
        world.resource::<PapyrusModEventRuntime>().pending,
        vec![small],
        "the event past the budget stays queued"
    );
    papyrus_provider_system(&world, 0.0);
    assert!(world.resource::<PapyrusModEventRuntime>().pending.is_empty());
}

/// #6025: a saved `Wait` above the lowering bound is refused on validation,
/// not just a non-finite or negative one.
#[test]
fn saved_wait_above_the_lowering_bound_is_rejected() {
    let wait = |seconds: f32| vec![PapyrusProviderStatement::Wait { seconds }];
    let catalog = catalog();
    assert!(validate_provider_statements(&wait(MAX_PROVIDER_WAIT_SECONDS), &catalog, 0).is_ok());
    assert!(validate_provider_statements(&wait(3.0e38), &catalog, 0).is_err());
    assert!(validate_provider_statements(&wait(-1.0), &catalog, 0).is_err());
    assert!(validate_provider_statements(&wait(f32::NAN), &catalog, 0).is_err());
}
