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

/// Regression for #3939. `PapyrusProviderRuntime::default()` publishes a
/// non-empty `engine_compatibility()` catalog with `callback: None` — and so
/// does every one of the thirteen `?` exits in `load_requested_extensions`
/// that return before the callback is synced, a startup error `App::new`
/// logs and continues past.
///
/// Lowering against that catalog turns `Game.GetModCount()` into a provider
/// *barrier* rather than declining, and dispatch then drops the barrier and
/// its tail after the fragment prefix has already mutated quest state. The
/// read is what enforces the pairing now: no callback, no catalog.
#[test]
fn a_catalog_with_no_live_callback_is_not_servable() {
    let runtime = PapyrusProviderRuntime::default();

    // The raw accessor still reports what was published — this is not a
    // claim that the catalog is empty, only that it cannot be served.
    assert!(
        runtime.catalog().resolve("Game", "GetModCount").is_some(),
        "precondition: Default publishes the engine-compatibility catalog"
    );
    assert!(
        runtime.callback().is_none(),
        "precondition: Default publishes no callback"
    );

    let servable = runtime.servable_catalog();
    assert!(
        servable.resolve("Game", "GetModCount").is_none(),
        "an alias the dispatcher has no callback to serve must not reach \
         lowering — it becomes a barrier that strands its own fragment tail \
         (#3939)"
    );
    // An empty catalog is exactly "no providers" at the seam: the call
    // yields no `TypedPapyrusProviderCall`, so nothing becomes a barrier.
    // (It reports `UnknownFunction` rather than `Ok(None)` because `Game` is
    // a *statically* known provider identifier — both arms decline in
    // `classify_effect_or_provider_call`, which is the outcome that matters.)
    assert!(
        !matches!(
            lower_provider_call(&expression("Game.GetModCount()"), &servable),
            Ok(Some(_))
        ),
        "an unservable alias must not lower to a provider call"
    );
}

/// The behaviour #3939 is actually about, at the fragment level: the same
/// body lowers to a barrier plus its tail against a live catalog, and
/// declines *wholesale* against the unservable one — rather than lowering to
/// a barrier that dispatch later drops along with the `SetStage` behind it,
/// after the prefix has already run.
#[test]
fn a_provider_fragment_declines_wholesale_when_the_catalog_is_not_servable() {
    use crate::translate::effects::lower_fragment_with_quest_properties_and_providers;
    use byroredux_papyrus::parse_script;

    let (script, errors) = parse_script(
        "ScriptName QF extends Quest\n\
         Function Fragment_0()\n\
         Game.GetModByName(\"Update.esm\")\n\
         Self.SetStage(20)\n\
         EndFunction\n",
    )
    .expect("fixture parses");
    assert!(errors.is_empty());
    let body = script
        .body
        .iter()
        .find_map(|item| match &item.node {
            byroredux_papyrus::ast::ScriptItem::Function(function) => Some(function.body.clone()),
            _ => None,
        })
        .expect("the fixture declares one function");
    let body = body.as_slice();

    let live = PapyrusProviderCatalog::engine_compatibility();
    let lowered =
        lower_fragment_with_quest_properties_and_providers(body, &Default::default(), Some(&live))
            .expect("a servable catalog claims the provider call");
    assert!(
        lowered.len() > 1,
        "precondition: the live catalog lowers the barrier AND its tail — \
         the tail is what dispatch strands"
    );

    let unservable = PapyrusProviderRuntime::default().servable_catalog();
    assert_eq!(
        lower_fragment_with_quest_properties_and_providers(
            body,
            &Default::default(),
            Some(&unservable)
        ),
        None,
        "with no callback to serve it, the fragment must decline at the \
         boundary — inert and consistent — instead of advancing its prefix \
         and stranding everything after the barrier (#3939)"
    );
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

/// #6021 — a script with provider handlers *and* handlers the program cannot
/// run is declined whole at the translation seam (no recognizer claims it),
/// rather than attached to execute half-way. The program itself still records
/// which handlers it left unlowered.
#[test]
fn a_script_with_unlowered_non_provider_handlers_is_declined_whole() {
    let translation = crate::translate_pex_detailed_with_providers(
        &mixed_provider_pex_bytes(),
        byroredux_plugin::esm::reader::GameKind::Skyrim,
        None,
        None,
        &catalog(),
    );
    assert!(translation.recognized.is_none(), "fixture must not be recognized");
    assert!(
        translation.provider_program.is_none(),
        "the provider handlers must not run without the OnUpdate handler"
    );
    assert_eq!(
        translation.provider_error,
        Some(PapyrusProviderProgramError::MixedScript(vec!["OnUpdate".to_owned()]))
    );

    // The same script without the extra handler is untouched.
    let pure = crate::translate_pex_detailed_with_providers(
        &provider_call_pex_bytes(),
        byroredux_plugin::esm::reader::GameKind::Skyrim,
        None,
        None,
        &catalog(),
    );
    assert_eq!(pure.provider_error, None);
    assert!(pure.provider_program.unwrap().unlowered_handlers().is_empty());

    // Source form: the lowering reports the handler it did not lower, and an
    // empty non-provider handler is not "behaviour left behind".
    let source = r#"
        ScriptName Fixture
        Event OnInit()
            WeatherNative.WeatherAt(1, "x")
        EndEvent
        Event OnUpdate()
            Utility.RandomInt(0, 1)
        EndEvent
        Event OnHit(ObjectReference akAggressor)
        EndEvent
    "#;
    let (script, errors) = parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let program = lower_provider_program(&script, &catalog()).unwrap().unwrap();
    assert_eq!(program.unlowered_handlers(), ["OnUpdate".to_owned()]);
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

/// #5656 — an `elseif` chain lowers to one wrapper `If` per clause, so
/// the k-th clause body executes (and is re-validated by
/// `validate_provider_statements` on every dispatch) at nesting depth
/// k + 2. Lowering used to pass every clause `depth + 1`, so a chain
/// deeper than `MAX_PROVIDER_HANDLER_NESTING` lowered fine and was then
/// rejected with a warn on every dispatch — the handler never ran.
/// Lowering now counts the chain: the too-deep chain is declined up
/// front, and a chain just inside the bound both lowers and validates.
#[test]
fn elseif_chains_count_into_the_nesting_bound_at_lowering() {
    let source = |clauses: usize, else_body: bool| {
        let mut source = String::from(
            "ScriptName Fixture\nEvent OnLoad()\nIf true\nWeatherNative.WeatherAt(1)\n",
        );
        for _ in 0..clauses {
            source.push_str("ElseIf true\nWeatherNative.WeatherAt(2)\n");
        }
        if else_body {
            source.push_str("Else\nWeatherNative.WeatherAt(3)\n");
        }
        source.push_str("EndIf\nEndEvent");
        source
    };
    let lower = |clauses: usize, else_body: bool| {
        let (script, errors) = parse_script(&source(clauses, else_body)).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        lower_provider_program(&script, &catalog())
    };

    // Clause k runs at depth k + 2, so `MAX - 1` clauses (k ≤ MAX - 2)
    // fit the bound with or without a trailing `Else` (depth MAX).
    for else_body in [false, true] {
        let program = lower(MAX_PROVIDER_HANDLER_NESTING - 1, else_body)
            .unwrap_or_else(|error| panic!("chain inside the bound must lower: {error:?}"))
            .expect("a provider-bearing handler");
        let statements = program.handler(PapyrusProviderEvent::OnLoad);
        assert!(
            super::super::execute::validate_provider_statements(statements, &catalog(), 0).is_ok(),
            "a chain that lowers within the bound must also pass the dispatch-time validator"
        );
    }

    // One clause more puts the last body at depth MAX + 1: lowering must
    // refuse it, not hand the runtime a program it would reject forever.
    assert_eq!(
        lower(MAX_PROVIDER_HANDLER_NESTING, false),
        Err(PapyrusProviderProgramError::NestingTooDeep)
    );
    // The trailing `Else` runs inside every wrapper — same bound.
    assert_eq!(
        lower(MAX_PROVIDER_HANDLER_NESTING, true),
        Err(PapyrusProviderProgramError::NestingTooDeep)
    );
}

/// #5657 — `Utility.Wait` is capped at lowering: `3.0e38` is a finite
/// `f32`, so it used to lower into a continuation that can never expire,
/// holding one of the 4 096 queue slots (which persist with the save and
/// whose overflow drops the *newest* entry) for the rest of the session.
/// A wait over one game day now declines the handler — the decline-safe
/// outcome, since no session can satisfy it.
#[test]
fn unbounded_utility_waits_decline_at_lowering() {
    let lower = |wait: &str| {
        let source = format!(
            "ScriptName Fixture\nEvent OnLoad()\nWeatherNative.WeatherAt(1)\nUtility.Wait({wait})\nEndEvent"
        );
        let (script, errors) = parse_script(&source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        lower_provider_program(&script, &catalog())
    };

    // One game day exactly — the documented ceiling — still lowers.
    assert!(
        lower("86400.0").unwrap().is_some(),
        "the cap is inclusive: a one-game-day wait is the ceiling, not over it"
    );
    // The `.psc` lexer has no exponent form, but the PEX decompiler's
    // float printing reaches any f32 bit pattern with full decimal
    // expansion, and plain integer literals reach i64::MAX — both far
    // over the cap.
    for over in [
        "86400.1",
        "340000000000000000000000000000000000000.0",
        "9223372036854775807",
    ] {
        assert_eq!(
            lower(over),
            Err(PapyrusProviderProgramError::UnsupportedStatement),
            "Utility.Wait({over}) must decline at lowering"
        );
    }
}

/// #6025 — a `Game.GetPlayer().Next().Next()…` receiver chain used to restart
/// the nesting counter at each call boundary, so the advertised
/// `MAX_PROVIDER_HANDLER_NESTING` did not bound it. Lowering now counts each
/// receiver link, and the dispatch-time validator agrees on what is in bounds.
#[test]
fn receiver_chain_depth_is_bounded_by_the_handler_nesting_cap() {
    let next = ScriptFunctionDeclaration {
        id: ScriptFunctionId::new("next-object").unwrap(),
        component: ComponentId::new("runtime").unwrap(),
        parameters: vec![ScriptParameterDeclaration {
            id: ScriptParameterId::new("receiver").unwrap(),
            value_type: ScriptValueType::Entity,
            optional: false,
        }],
        result: Some(ScriptResultDeclaration {
            value_type: ScriptValueType::Entity,
            optional: true,
        }),
        papyrus: Some(PapyrusFunctionAlias {
            provider: "ObjectReference".to_owned(),
            function: "Next".to_owned(),
        }),
        description: "A chainable object receiver".to_owned(),
    };
    let mut catalog = object_catalog();
    catalog
        .insert(&ExtensionId::new("org.example.object").unwrap(), &next)
        .unwrap();
    let lower = |links: usize| {
        let source = format!(
            "ScriptName Fixture\nEvent OnLoad()\n  Game.GetPlayer(){}.Touch(7)\nEndEvent\n",
            ".Next()".repeat(links)
        );
        let (script, errors) = parse_script(&source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        lower_provider_program(&script, &catalog)
    };

    let within = lower(MAX_PROVIDER_HANDLER_NESTING - 4)
        .unwrap_or_else(|error| panic!("a chain inside the bound must lower: {error:?}"))
        .expect("a provider-bearing handler");
    assert!(
        super::super::execute::validate_provider_statements(
            within.handler(PapyrusProviderEvent::OnLoad),
            &catalog,
            0
        )
        .is_ok(),
        "a chain that lowers within the bound must also pass the dispatch-time validator"
    );
    assert!(
        lower(MAX_PROVIDER_HANDLER_NESTING + 8).is_err(),
        "a chain past the cap must be refused at lowering, not left to the expression-depth caps"
    );
}
