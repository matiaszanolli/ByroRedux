//! Execution tests — these drive a live `World`.

use super::*;

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
