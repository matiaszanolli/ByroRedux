//! Lowering tests — pure, no `World`.

use super::*;

#[test]
fn self_receiver_lowers_to_an_explicit_entity_argument() {
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
    let [PapyrusProviderStatement::Call(call)] = program.handler(PapyrusProviderEvent::OnInit)
    else {
        panic!("expected one self provider call");
    };
    assert_eq!(
        call.receiver,
        Some(Box::new(PapyrusProviderArgument::Local {
            name: PAPYRUS_SELF_LOCAL.to_owned(),
            value_type: ScriptValueType::Entity,
        }))
    );
    assert_eq!(
        call.arguments,
        [PapyrusProviderArgument::Literal(ScriptValue::Integer(7))]
    );
}

#[test]
fn self_receiver_handlers_reject_latent_owner_use_until_continuations_persist_it() {
    let source = r#"
        ScriptName Fixture
        Event OnInit()
            self.Touch(7)
            Utility.Wait(1.0)
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(
        lower_provider_program(&script, &self_catalog()),
        Err(PapyrusProviderProgramError::UnsupportedStatement)
    );
}

#[test]
fn typed_object_receiver_lowers_to_an_explicit_entity_argument() {
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
    let [PapyrusProviderStatement::Call(call)] =
        program.handler(PapyrusProviderEvent::OnTriggerEnter)
    else {
        panic!("expected one typed object provider call");
    };
    assert_eq!(
        call.receiver,
        Some(Box::new(PapyrusProviderArgument::Local {
            name: "akactionref".to_owned(),
            value_type: ScriptValueType::Entity,
        }))
    );
    assert_eq!(
        call.arguments,
        [PapyrusProviderArgument::Literal(ScriptValue::Integer(7))]
    );
    assert_eq!(
        call.route.qualified_name(),
        "ext.org.example.object.touch-object"
    );
}

#[test]
fn receiver_expression_lowers_nested_game_player_call() {
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
    let [PapyrusProviderStatement::Call(call)] = program.handler(PapyrusProviderEvent::OnLoad)
    else {
        panic!("expected one receiver-expression provider call");
    };
    assert_eq!(
        call.route.qualified_name(),
        "ext.org.example.object.touch-object"
    );
    assert_eq!(call.result_object_type, None);
    assert_eq!(
        call.receiver,
        Some(Box::new(PapyrusProviderArgument::Value {
            value: Box::new(PapyrusProviderValue::Call(PapyrusProviderInvocation {
                route: object_catalog()
                    .resolve("Game", "GetPlayer")
                    .unwrap()
                    .clone(),
                receiver: None,
                arguments: Vec::new(),
                result: Some(ScriptResultDeclaration {
                    value_type: ScriptValueType::Entity,
                    optional: true,
                }),
                result_object_type: Some(PapyrusProviderObjectType::ObjectReference),
            })),
            value_type: ScriptValueType::Entity,
        }))
    );
}

#[test]
fn static_calls_resolve_case_insensitively_and_reorder_named_arguments() {
    let call = lower_provider_call(
        &expression("WEATHERNATIVE.weatherat(fallback = \"clear\", day = 4)"),
        &catalog(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        call.route.qualified_name(),
        "ext.org.example.weather.weather-at"
    );
    assert_eq!(
        call.arguments,
        [
            ScriptValue::Integer(4),
            ScriptValue::String("clear".to_owned())
        ]
    );
}

#[test]
fn known_providers_fail_closed_on_unknown_functions_and_bad_arguments() {
    assert!(matches!(
        lower_provider_call(&expression("WeatherNative.Missing(4)"), &catalog()),
        Err(PapyrusProviderLowerError::UnknownFunction { .. })
    ));
    assert!(matches!(
        lower_provider_call(&expression("WeatherNative.WeatherAt(\"four\")"), &catalog()),
        Err(PapyrusProviderLowerError::UnsupportedArgument { .. })
    ));
    assert!(matches!(
        lower_provider_call(
            &expression("WeatherNative.WeatherAt(fallback = \"x\")"),
            &catalog()
        ),
        Err(PapyrusProviderLowerError::MissingParameter(_))
            | Err(PapyrusProviderLowerError::InvalidArguments(_))
    ));
}

#[test]
fn recognized_extender_call_without_an_executable_route_fails_closed() {
    let empty = PapyrusProviderCatalog::default();
    assert_eq!(
        lower_provider_call(&expression("SKSE.GetVersion()"), &empty),
        Err(PapyrusProviderLowerError::UnknownFunction {
            provider: "SKSE".to_owned(),
            function: "GetVersion".to_owned(),
        })
    );

    let source = r#"
        ScriptName Fixture
        Event OnLoad()
            SKSE.UnknownNative()
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(
        lower_provider_program(&script, &empty),
        Err(PapyrusProviderProgramError::Call(
            PapyrusProviderLowerError::UnknownFunction {
                provider: "SKSE".to_owned(),
                function: "UnknownNative".to_owned(),
            }
        ))
    );
}

#[test]
fn engine_compatibility_catalog_lowers_read_only_input_aliases() {
    let catalog = PapyrusProviderCatalog::engine_compatibility();
    let key = lower_provider_call(&expression("Input.GetMappedKey(\"Forward\")"), &catalog)
        .unwrap()
        .unwrap();
    assert_eq!(
        key.route.qualified_name(),
        byroredux_sdk::compatibility::PAPYRUS_INPUT_GET_MAPPED_KEY_ROUTE
    );
    assert_eq!(key.arguments, [ScriptValue::String("Forward".to_owned())]);
    let control = lower_provider_call(&expression("Input.GetMappedControl(17)"), &catalog)
        .unwrap()
        .unwrap();
    assert_eq!(
        control.route.qualified_name(),
        byroredux_sdk::compatibility::PAPYRUS_INPUT_GET_MAPPED_CONTROL_ROUTE
    );
    assert_eq!(control.arguments, [ScriptValue::Integer(17)]);
    assert_eq!(
        catalog
            .resolve("Input", "GetMappedKey")
            .unwrap()
            .declaration()
            .parameters
            .len(),
        2
    );
    assert!(matches!(
        lower_provider_call(&expression("Input.TapKey(17)"), &catalog),
        Err(PapyrusProviderLowerError::UnknownFunction { .. })
    ));
    let menu = lower_provider_call(&expression("UI.IsMenuOpen(\"InventoryMenu\")"), &catalog)
        .unwrap()
        .unwrap();
    assert_eq!(
        menu.route.qualified_name(),
        byroredux_sdk::compatibility::PAPYRUS_UI_IS_MENU_OPEN_ROUTE
    );
    assert_eq!(
        menu.arguments,
        [ScriptValue::String("InventoryMenu".to_owned())]
    );
    assert!(matches!(
        lower_provider_call(
            &expression("UI.IsMenuRegistered(\"InventoryMenu\")"),
            &catalog
        ),
        Err(PapyrusProviderLowerError::UnknownFunction { .. })
    ));
}

#[test]
fn engine_compatibility_catalog_lowers_exact_game_storage_and_container_aliases() {
    let mut catalog = PapyrusProviderCatalog::engine_compatibility();
    assert!(PapyrusProviderRuntime::default()
        .catalog()
        .resolve("Game", "GetModCount")
        .is_some());
    let call = lower_provider_call(&expression("Game.GetModByName(\"Update.esm\")"), &catalog)
        .unwrap()
        .unwrap();
    assert_eq!(
        call.route.qualified_name(),
        byroredux_sdk::compatibility::PAPYRUS_GAME_GET_MOD_BY_NAME_ROUTE
    );
    assert_eq!(
        call.arguments,
        [ScriptValue::String("Update.esm".to_owned())]
    );
    let form_from_file = lower_provider_call(
        &expression("Game.GetFormFromFile(4660, \"Update.esm\")"),
        &catalog,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        form_from_file.route.qualified_name(),
        byroredux_sdk::compatibility::PAPYRUS_GAME_GET_FORM_FROM_FILE_ROUTE
    );
    assert_eq!(
        form_from_file.arguments,
        [
            ScriptValue::Integer(4660),
            ScriptValue::String("Update.esm".to_owned()),
        ]
    );
    let player = lower_provider_call(&expression("Game.GetPlayer()"), &catalog)
        .unwrap()
        .unwrap();
    assert_eq!(
        player.route.qualified_name(),
        byroredux_sdk::compatibility::PAPYRUS_GAME_GET_PLAYER_ROUTE
    );
    assert!(player.arguments.is_empty());
    assert_eq!(
        player.result,
        Some(ScriptResultDeclaration {
            value_type: ScriptValueType::Entity,
            optional: true,
        })
    );
    let storage = lower_provider_call(
        &expression("StorageUtil.GetIntValue(None, \"Score\", -1)"),
        &catalog,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        storage.route.qualified_name(),
        byroredux_sdk::compatibility::PAPYRUS_STORAGE_UTIL_GET_INT_VALUE_ROUTE
    );
    assert_eq!(
        storage.arguments,
        [
            ScriptValue::None,
            ScriptValue::String("Score".to_owned()),
            ScriptValue::Integer(-1),
        ]
    );
    let float = lower_provider_call(
        &expression("StorageUtil.AdjustFloatValue(None, \"Weight\", 0.5)"),
        &catalog,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        float.route.qualified_name(),
        byroredux_sdk::compatibility::PAPYRUS_STORAGE_UTIL_ADJUST_FLOAT_VALUE_ROUTE
    );
    assert_eq!(
        float.arguments,
        [
            ScriptValue::None,
            ScriptValue::String("Weight".to_owned()),
            ScriptValue::Float(0.5),
        ]
    );
    let form = lower_provider_call(
        &expression("StorageUtil.SetFormValue(None, \"Owner\", None)"),
        &catalog,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        form.route.qualified_name(),
        byroredux_sdk::compatibility::PAPYRUS_STORAGE_UTIL_SET_FORM_VALUE_ROUTE
    );
    assert_eq!(
        form.arguments,
        [
            ScriptValue::None,
            ScriptValue::String("Owner".to_owned()),
            ScriptValue::None,
        ]
    );
    let pluck = lower_provider_call(
        &expression("StorageUtil.PluckFormValue(None, \"Owner\", None)"),
        &catalog,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        pluck.route.qualified_name(),
        byroredux_sdk::compatibility::PAPYRUS_STORAGE_UTIL_PLUCK_FORM_VALUE_ROUTE
    );
    let list = lower_provider_call(
        &expression("StorageUtil.IntListAdd(None, \"Recent\", 7, false)"),
        &catalog,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        list.route.qualified_name(),
        "byro.storage.compat.storage-util.list-int-add"
    );
    assert_eq!(
        list.arguments,
        [
            ScriptValue::None,
            ScriptValue::String("Recent".to_owned()),
            ScriptValue::Integer(7),
            ScriptValue::Boolean(false),
        ]
    );
    let list_pluck = lower_provider_call(
        &expression("StorageUtil.StringListPluck(None, \"Labels\", 2, \"missing\")"),
        &catalog,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        list_pluck.route.qualified_name(),
        "byro.storage.compat.storage-util.list-string-pluck"
    );
    assert_eq!(
        list_pluck.arguments,
        [
            ScriptValue::None,
            ScriptValue::String("Labels".to_owned()),
            ScriptValue::Integer(2),
            ScriptValue::String("missing".to_owned()),
        ]
    );
    let list_remove = lower_provider_call(
        &expression("StorageUtil.FormListRemove(None, \"Owners\", None, true)"),
        &catalog,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        list_remove.route.qualified_name(),
        "byro.storage.compat.storage-util.list-form-remove"
    );
    assert_eq!(
        list_remove.arguments,
        [
            ScriptValue::None,
            ScriptValue::String("Owners".to_owned()),
            ScriptValue::None,
            ScriptValue::Boolean(true),
        ]
    );
    let list_resize = lower_provider_call(
        &expression("StorageUtil.FloatListResize(None, \"Ratios\", 4, 1.5)"),
        &catalog,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        list_resize.route.qualified_name(),
        "byro.storage.compat.storage-util.list-float-resize"
    );
    assert_eq!(
        list_resize.arguments,
        [
            ScriptValue::None,
            ScriptValue::String("Ratios".to_owned()),
            ScriptValue::Integer(4),
            ScriptValue::Float(1.5),
        ]
    );
    let list_sort = lower_provider_call(
        &expression("StorageUtil.FormListSort(None, \"Owners\")"),
        &catalog,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        list_sort.route.qualified_name(),
        "byro.storage.compat.storage-util.list-form-sort"
    );
    assert!(list_sort.result.is_none());
    let list_random = lower_provider_call(
        &expression("StorageUtil.FormListRandom(None, \"Owners\")"),
        &catalog,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        list_random.route.qualified_name(),
        "byro.storage.compat.storage-util.list-form-random"
    );
    assert_eq!(
        list_random.arguments,
        [ScriptValue::None, ScriptValue::String("Owners".to_owned()),]
    );
    let list_array = lower_provider_call(
        &expression("StorageUtil.IntListToArray(None, \"Numbers\")"),
        &catalog,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        list_array.route.qualified_name(),
        "byro.storage.compat.storage-util.list-int-to-array"
    );
    assert_eq!(
        list_array.result.unwrap().value_type,
        ScriptValueType::IntegerArray
    );
    let list_filter = lower_provider_call(
        &expression("StorageUtil.FormListFilterByType(None, \"Owners\", 41, false)"),
        &catalog,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        list_filter.route.qualified_name(),
        byroredux_sdk::compatibility::PAPYRUS_STORAGE_UTIL_FORM_FILTER_BY_TYPE_ROUTE
    );
    assert_eq!(
        list_filter.arguments,
        [
            ScriptValue::None,
            ScriptValue::String("Owners".to_owned()),
            ScriptValue::Integer(41),
            ScriptValue::Boolean(false),
        ]
    );
    assert_eq!(
        list_filter.result.unwrap().value_type,
        ScriptValueType::FormArray
    );
    let prefix_route = catalog.resolve("StorageUtil", "CountAllPrefix").unwrap();
    assert_eq!(
        prefix_route.declaration().parameters[0].id.as_str(),
        "prefix"
    );
    let prefix_expression = expression("StorageUtil.CountAllPrefix(\"my_mod.\")");
    let prefix = lower_provider_call(&prefix_expression, &catalog)
        .unwrap()
        .unwrap();
    assert_eq!(
        prefix.route.qualified_name(),
        "byro.storage.compat.storage-util.prefix-count-all"
    );
    assert_eq!(
        prefix.arguments,
        [ScriptValue::String("my_mod.".to_owned())]
    );
    let container = lower_provider_call(&expression("JArray.getInt(4, -1, 7)"), &catalog)
        .unwrap()
        .unwrap();
    assert_eq!(
        container.route.qualified_name(),
        "byro.legacy-containers.compat.jarray-get-int"
    );
    assert_eq!(
        container.arguments,
        [
            ScriptValue::Integer(4),
            ScriptValue::Integer(-1),
            ScriptValue::Integer(7),
        ]
    );
    let mod_event = lower_provider_call(&expression("ModEvent.PushString(7, \"ready\")"), &catalog)
        .unwrap()
        .unwrap();
    assert_eq!(
        mod_event.route.qualified_name(),
        "byro.events.compat.mod-event.mod-event-push-string"
    );
    assert_eq!(
        mod_event.arguments,
        [
            ScriptValue::Integer(7),
            ScriptValue::String("ready".to_owned()),
        ]
    );
    assert!(matches!(
        lower_provider_call(&expression("ModEvent.PushForm(7)"), &catalog),
        Err(PapyrusProviderLowerError::MissingParameter(_))
    ));
    assert!(matches!(
        catalog.insert(
            &ExtensionId::new("org.example.shadow").unwrap(),
            &papyrus_game_content_declarations()
                .into_iter()
                .find(|function| {
                    function.route
                        == byroredux_sdk::compatibility::PAPYRUS_GAME_GET_MOD_BY_NAME_ROUTE
                })
                .unwrap()
                .declaration,
        ),
        Err(PapyrusProviderCatalogError::DuplicateAlias { .. })
    ));
}

#[test]
fn unrelated_calls_are_left_for_other_translators() {
    assert_eq!(
        lower_provider_call(&expression("Utility.Wait(1.0)"), &catalog()).unwrap(),
        None
    );
}

#[test]
fn aliases_are_unique_across_principals() {
    let mut catalog = catalog();
    let error = catalog
        .insert(
            &ExtensionId::new("org.example.other").unwrap(),
            &declaration(),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        PapyrusProviderCatalogError::DuplicateAlias { .. }
    ));
}

#[test]
fn entity_conditions_reject_ordered_comparisons() {
    let source = r#"
        ScriptName Fixture
        Event OnLoad()
            ObjectReference player
            player = Game.GetPlayer()
            If player < player
                WeatherNative.WeatherAt(1, "unexpected")
            EndIf
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(
        lower_provider_program(&script, &PapyrusProviderCatalog::engine_compatibility()),
        Err(PapyrusProviderProgramError::UnsupportedStatement)
    );
}

#[test]
fn provider_expressions_reject_mixed_numeric_types_and_runtime_faults() {
    let source = r#"
        ScriptName Fixture
        Event OnLoad()
            Int count
            count = Game.GetModCount() + 1.5
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(
        lower_provider_program(&script, &PapyrusProviderCatalog::engine_compatibility()),
        Err(PapyrusProviderProgramError::UnsupportedStatement)
    );

    assert!(apply_provider_arithmetic(
        ScriptValue::Integer(i64::MAX),
        PapyrusProviderArithmetic::Add,
        ScriptValue::Integer(1),
    )
    .is_err());
    assert!(apply_provider_arithmetic(
        ScriptValue::Integer(1),
        PapyrusProviderArithmetic::Div,
        ScriptValue::Integer(0),
    )
    .is_err());
    assert!(apply_provider_arithmetic(
        ScriptValue::String("x".repeat(4 * 1024)),
        PapyrusProviderArithmetic::StrCat,
        ScriptValue::String("y".to_owned()),
    )
    .is_err());
}

#[test]
fn provider_bearing_handler_rejects_unsupported_statements_as_a_unit() {
    let source = r#"
        ScriptName Fixture
        Event OnLoad()
            While WeatherNative.IsStorm()
                WeatherNative.WeatherAt(4)
            EndWhile
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(
        lower_provider_program(&script, &catalog()),
        Err(PapyrusProviderProgramError::UnsupportedStatement)
    );
}

#[test]
fn reference_event_parameters_do_not_cross_latent_waits() {
    let source = r#"
        ScriptName Fixture
        Event OnTriggerEnter(ObjectReference akActionRef)
            WeatherNative.InspectEntity(akActionRef)
            Utility.Wait(1.0)
            WeatherNative.WeatherAt(1, "after")
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(
        lower_provider_program(&script, &catalog()),
        Err(PapyrusProviderProgramError::UnsupportedStatement)
    );
}

#[test]
fn byte_level_pex_static_call_lowers_to_the_same_provider_route() {
    let translation = crate::translate_pex_detailed_with_providers(
        &provider_call_pex_bytes(),
        byroredux_plugin::esm::reader::GameKind::Skyrim,
        None,
        None,
        &catalog(),
    );
    assert_eq!(translation.provider_error, None);
    let program = translation.provider_program.unwrap();
    let [PapyrusProviderStatement::Call(call)] = program.handler(PapyrusProviderEvent::OnLoad)
    else {
        panic!("expected one lowered provider call");
    };
    assert_eq!(
        call.route.qualified_name(),
        "ext.org.example.weather.weather-at"
    );
    assert_eq!(
        call.arguments,
        [
            PapyrusProviderArgument::Literal(ScriptValue::Integer(4)),
            PapyrusProviderArgument::Literal(ScriptValue::String("clear".to_owned()))
        ]
    );
}

#[test]
fn byte_level_pex_instance_send_mod_event_lowers_with_defaults() {
    let translation = crate::translate_pex_detailed_with_providers(
        &send_mod_event_pex_bytes(),
        byroredux_plugin::esm::reader::GameKind::Skyrim,
        None,
        None,
        &catalog(),
    );
    assert_eq!(translation.provider_error, None);
    let program = translation.provider_program.unwrap();
    let [PapyrusProviderStatement::SendModEvent {
        event_name,
        string_arg,
        number_arg,
        sender,
    }] = program.handler(PapyrusProviderEvent::OnLoad)
    else {
        panic!("expected one lowered instance SendModEvent call");
    };
    assert_eq!(
        event_name,
        &PapyrusProviderArgument::Literal(ScriptValue::String("ByroReady".to_owned()))
    );
    assert_eq!(
        string_arg,
        &PapyrusProviderArgument::Literal(ScriptValue::String(String::new()))
    );
    assert_eq!(
        number_arg,
        &PapyrusProviderArgument::Literal(ScriptValue::Float(0.0))
    );
    assert_eq!(sender, &PapyrusModEventSender::Owner);
}
