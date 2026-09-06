//! Compatibility-layer regression tests.

use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{PluginInfo, PluginKind};

    fn classic_catalog(names: &[&str]) -> ContentCatalog {
        ContentCatalog::new(
            names
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    PluginInfo::new(
                        *name,
                        (index as u128 + 1).to_be_bytes(),
                        PluginKind::Regular,
                    )
                    .unwrap()
                })
                .collect(),
        )
        .unwrap()
    }

    #[test]
    fn storage_and_events_map_to_existing_semantic_services() {
        let storage = classify_static_call("storageutil", "GetIntValue").unwrap();
        assert_eq!(storage.disposition, CompatibilityDisposition::Native);
        assert_eq!(storage.service, Some(PRINCIPAL_STORAGE_SERVICE));
        let event = classify_method_call("RegisterForModEvent").unwrap();
        assert_eq!(event.service, Some(EVENT_SERVICE));
    }

    #[test]
    fn input_mapping_aliases_are_read_only_bounded_and_case_insensitive() {
        let bindings = [
            PapyrusInputBinding {
                control: "Forward".to_owned(),
                device_type: 0,
                keycode: 17,
            },
            PapyrusInputBinding {
                control: "Forward".to_owned(),
                device_type: 1,
                keycode: 1,
            },
        ];
        assert_eq!(
            adapt_papyrus_input_get_mapped_key(&bindings, "forward", 0xff),
            17
        );
        assert_eq!(
            adapt_papyrus_input_get_mapped_key(&bindings, "FORWARD", 1),
            1
        );
        assert_eq!(
            adapt_papyrus_input_get_mapped_key(&bindings, "missing", 0),
            PAPYRUS_INPUT_UNBOUND_KEY
        );
        assert_eq!(
            adapt_papyrus_input_get_mapped_key(&bindings, "forward", 9),
            PAPYRUS_INPUT_UNBOUND_KEY
        );
        assert_eq!(
            adapt_papyrus_input_get_mapped_control(&bindings, 17),
            "Forward"
        );
        assert_eq!(adapt_papyrus_input_get_mapped_control(&bindings, 99), "");

        let declarations = papyrus_input_declarations();
        assert_eq!(declarations.len(), 2);
        assert!(declarations
            .iter()
            .all(|declaration| declaration.declaration.validate().is_ok()));
        assert!(declarations[0].declaration.parameters[1].optional);
        assert_eq!(
            source_alias("input", "getmappedkey").unwrap().service,
            INPUT_SERVICE
        );
        assert_eq!(
            classify_static_call("INPUT", "GetMappedControl")
                .unwrap()
                .disposition,
            CompatibilityDisposition::Native
        );
        assert_eq!(
            classify_static_call("Input", "TapKey").unwrap().disposition,
            CompatibilityDisposition::Unsupported
        );
    }

    #[test]
    fn ui_menu_alias_reads_only_the_active_visible_menu() {
        let snapshot = PapyrusUiMenuSnapshot {
            active_menu: Some("InventoryMenu".to_owned()),
            visible: true,
        };
        assert!(adapt_papyrus_ui_is_menu_open(&snapshot, "InventoryMenu"));
        assert!(!adapt_papyrus_ui_is_menu_open(&snapshot, "inventorymenu"));
        assert!(!adapt_papyrus_ui_is_menu_open(&snapshot, "PauseMenu"));
        assert!(!adapt_papyrus_ui_is_menu_open(&snapshot, ""));
        assert!(!adapt_papyrus_ui_is_menu_open(
            &PapyrusUiMenuSnapshot {
                visible: false,
                ..snapshot
            },
            "InventoryMenu"
        ));

        let declarations = papyrus_ui_declarations();
        assert_eq!(declarations.len(), 1);
        declarations[0].declaration.validate().unwrap();
        assert_eq!(
            source_alias("ui", "ismenuopen").unwrap().service,
            UI_SERVICE
        );
        assert_eq!(
            classify_static_call("UI", "IsMenuOpen")
                .unwrap()
                .disposition,
            CompatibilityDisposition::Native
        );
        assert_eq!(
            classify_static_call("UI", "IsMenuRegistered")
                .unwrap()
                .disposition,
            CompatibilityDisposition::Unsupported
        );
    }

    #[test]
    fn legacy_obscript_version_probes_map_to_context_discovery() {
        let nvse = classify_obscript_command("getnvseversion").unwrap();
        assert_eq!(nvse.family, ExtenderFamily::Xnvse);
        assert_eq!(nvse.service, Some(CONTEXT_SERVICE));
        assert_eq!(nvse.disposition, CompatibilityDisposition::Mapped);
        let obse = classify_obscript_command("GetOBSERevision").unwrap();
        assert_eq!(obse.family, ExtenderFamily::Obse);
        assert_eq!(obse.service, Some(CONTEXT_SERVICE));
        assert_eq!(
            classify_obscript_command("GetNVSEUnknown")
                .unwrap()
                .disposition,
            CompatibilityDisposition::Unsupported
        );
        assert!(classify_obscript_command("GetActorValue").is_none());
    }

    #[test]
    fn legacy_load_order_commands_map_to_content_catalog_recipes() {
        let loaded = obscript_source_alias("ismodloaded").unwrap();
        assert_eq!(loaded.service, CONTENT_CATALOG_SERVICE);
        assert_eq!(loaded.operation, "content.find");
        assert_eq!(loaded.value_kind, "bool");

        let index = obscript_source_alias("GetModIndex").unwrap();
        assert_eq!(index.operation, "content.find-index");
        assert!(index.constraint.contains("255 sentinel"));

        assert_eq!(
            obscript_source_alias("GetNumLoadedMods").unwrap().operation,
            "content.count"
        );
        assert_eq!(
            obscript_source_alias("GetNumLoadedPlugins")
                .unwrap()
                .operation,
            "content.count"
        );
        assert_eq!(
            obscript_source_alias("GetNthModName").unwrap().operation,
            "content.plugin-name"
        );
        assert_eq!(
            classify_obscript_command("IsModLoaded").unwrap().service,
            Some(CONTENT_CATALOG_SERVICE)
        );
        assert_eq!(
            classify_obscript_command("IsModLoaded")
                .unwrap()
                .disposition,
            CompatibilityDisposition::Native
        );
        assert!(obscript_source_alias("GetSourceModIndex").is_none());
    }

    #[test]
    fn papyrus_game_content_aliases_preserve_regular_and_light_indices() {
        let catalog = ContentCatalog::new_with_dependencies(
            vec![
                PluginInfo::new("Skyrim.esm", 1_u128.to_be_bytes(), PluginKind::Regular).unwrap(),
                PluginInfo::new("Update.esm", 3_u128.to_be_bytes(), PluginKind::Regular).unwrap(),
                PluginInfo::new("Patch.esl", 2_u128.to_be_bytes(), PluginKind::Light).unwrap(),
            ],
            vec![vec![], vec![0], vec![1]],
        )
        .unwrap();

        assert_eq!(
            adapt_papyrus_game_get_mod_by_name(&catalog, "UPDATE.ESM"),
            1
        );
        assert_eq!(
            adapt_papyrus_game_get_mod_by_name(&catalog, "Patch.esl"),
            0x100
        );
        assert_eq!(
            adapt_papyrus_game_get_mod_by_name(&catalog, "Missing.esp"),
            255
        );
        assert_eq!(
            adapt_papyrus_game_get_form_from_file(&catalog, 0x1234, "UPDATE.ESM"),
            Some(FormRef::new(3_u128.to_be_bytes(), 0x1234))
        );
        assert_eq!(
            adapt_papyrus_game_get_form_from_file(&catalog, -1, "Update.esm"),
            None
        );
        assert_eq!(
            adapt_papyrus_game_get_form_from_file(&catalog, 0x1234, "Missing.esp"),
            None
        );
        assert_eq!(adapt_papyrus_game_get_mod_count(&catalog), 2);
        assert_eq!(adapt_papyrus_game_get_mod_name(&catalog, 1), "Update.esm");
        assert_eq!(
            adapt_papyrus_game_get_mod_name(&catalog, 0x100),
            "Patch.esl"
        );
        assert_eq!(adapt_papyrus_game_get_mod_name(&catalog, 255), "");
        assert_eq!(adapt_papyrus_game_get_mod_dependency_count(&catalog, 1), 1);
        assert_eq!(
            adapt_papyrus_game_get_mod_dependency_count(&catalog, 0x100),
            1
        );
        assert_eq!(adapt_papyrus_game_get_mod_dependency_count(&catalog, -1), 0);
        assert!(adapt_papyrus_game_is_plugin_installed(
            &catalog,
            "patch.ESL"
        ));
        assert_eq!(adapt_papyrus_game_get_light_mod_count(&catalog), 1);
        assert_eq!(
            adapt_papyrus_game_get_light_mod_by_name(&catalog, "Patch.esl"),
            0
        );
        assert_eq!(
            adapt_papyrus_game_get_light_mod_by_name(&catalog, "Skyrim.esm"),
            0xffff
        );
        assert_eq!(
            adapt_papyrus_game_get_light_mod_name(&catalog, 0),
            "Patch.esl"
        );
        assert_eq!(adapt_papyrus_game_get_light_mod_name(&catalog, -1), "");
        assert_eq!(
            adapt_papyrus_game_get_light_mod_dependency_count(&catalog, 0),
            1
        );
        assert_eq!(
            adapt_papyrus_game_get_light_mod_dependency_count(&catalog, 1),
            0
        );
        assert_eq!(
            adapt_papyrus_game_get_nth_light_mod_dependency(&catalog, 0, 0),
            1
        );
        assert_eq!(
            adapt_papyrus_game_get_nth_light_mod_dependency(&catalog, 0, 1),
            0
        );
        assert_eq!(
            adapt_papyrus_game_get_nth_light_mod_dependency(&catalog, -1, 0),
            0
        );
        let declarations = papyrus_game_content_declarations();
        assert_eq!(declarations.len(), 12);
        for declaration in declarations {
            declaration.declaration.validate().unwrap();
        }
        assert_eq!(
            classify_static_call("game", "getmodbyname")
                .unwrap()
                .disposition,
            CompatibilityDisposition::Native
        );
        assert_eq!(
            classify_static_call("game", "getformfromfile")
                .unwrap()
                .disposition,
            CompatibilityDisposition::Native
        );
        let player = classify_static_call("Game", "GetPlayer").unwrap();
        assert_eq!(player.disposition, CompatibilityDisposition::Native);
        assert_eq!(player.service, Some(WORLD_PROJECTION_SERVICE));
    }

    #[test]
    fn legacy_load_order_adapter_preserves_classic_results() {
        let catalog = classic_catalog(&["FalloutNV.esm", "Companion.esp"]);
        assert_eq!(
            adapt_legacy_obscript_load_order(
                &catalog,
                LegacyObscriptLoadOrderCall::IsModLoaded {
                    plugin: "companion.ESP".to_owned(),
                },
            ),
            Ok(LegacyObscriptLoadOrderResult::Bool(true))
        );
        assert_eq!(
            adapt_legacy_obscript_load_order(
                &catalog,
                LegacyObscriptLoadOrderCall::GetModIndex {
                    plugin: "missing.esp".to_owned(),
                },
            ),
            Ok(LegacyObscriptLoadOrderResult::Integer(255))
        );
        assert_eq!(
            adapt_legacy_obscript_load_order(
                &catalog,
                LegacyObscriptLoadOrderCall::GetNthModName { index: 1 },
            ),
            Ok(LegacyObscriptLoadOrderResult::String(
                "Companion.esp".to_owned()
            ))
        );
        assert_eq!(
            adapt_legacy_obscript_load_order(
                &catalog,
                LegacyObscriptLoadOrderCall::GetNthModName { index: -1 },
            ),
            Ok(LegacyObscriptLoadOrderResult::String(String::new()))
        );
    }

    #[test]
    fn legacy_load_order_adapter_rejects_unrepresentable_catalogs() {
        let names = (0..=LEGACY_OBSCRIPT_PLUGIN_LIMIT)
            .map(|index| format!("Plugin{index}.esp"))
            .collect::<Vec<_>>();
        let refs = names.iter().map(String::as_str).collect::<Vec<_>>();
        let catalog = classic_catalog(&refs);
        assert_eq!(
            adapt_legacy_obscript_load_order(
                &catalog,
                LegacyObscriptLoadOrderCall::GetNumLoadedMods,
            ),
            Err(LegacyObscriptLoadOrderError::PluginBudgetExceeded {
                actual: 256,
                maximum: 255,
            })
        );
    }

    #[test]
    fn storage_aliases_are_exact_global_scalar_operations() {
        let get = source_alias("storageutil", "getintvalue").unwrap();
        assert_eq!(get.service, PRINCIPAL_STORAGE_SERVICE);
        assert_eq!(get.operation, "storage.get");
        assert_eq!(get.value_kind, "signed");
        assert!(get.constraint.contains("ObjKey must be None"));

        let string = source_alias("StorageUtil", "HasStringValue").unwrap();
        assert_eq!(string.operation, "storage.get");
        assert_eq!(string.value_kind, "text");
        let set = source_alias("StorageUtil", "SetStringValue").unwrap();
        assert_eq!(set.operation, "storage.queue-set/delete");
        let unset = source_alias("StorageUtil", "UnsetIntValue").unwrap();
        assert_eq!(unset.operation, "storage.get+queue-delete");
        assert_eq!(
            source_alias("StorageUtil", "AdjustIntValue")
                .unwrap()
                .operation,
            "storage.get+queue-set/delete"
        );
        assert_eq!(
            source_alias("StorageUtil", "GetFloatValue")
                .unwrap()
                .value_kind,
            "float"
        );
        assert_eq!(
            source_alias("StorageUtil", "SetFormValue")
                .unwrap()
                .value_kind,
            "form"
        );
        assert_eq!(
            source_alias("StorageUtil", "PluckStringValue")
                .unwrap()
                .operation,
            "storage.get+queue-delete"
        );
        assert_eq!(
            source_alias("StorageUtil", "FormListAdd")
                .unwrap()
                .operation,
            "storage.array-get+queue-push"
        );
        assert_eq!(
            source_alias("StorageUtil", "FormListSet")
                .unwrap()
                .operation,
            "storage.array-get+queue-set"
        );
        assert_eq!(
            source_alias("StorageUtil", "FormListInsert")
                .unwrap()
                .operation,
            "storage.array-get+queue-replace"
        );
        assert_eq!(
            source_alias("StorageUtil", "FormListSort")
                .unwrap()
                .operation,
            "storage.array-get+queue-replace"
        );
        assert_eq!(
            source_alias("StorageUtil", "FormListRandom")
                .unwrap()
                .operation,
            "storage.array-get"
        );
        assert_eq!(
            source_alias("StorageUtil", "CountAllPrefix")
                .unwrap()
                .operation,
            "storage.prefix-count"
        );
        assert_eq!(
            source_alias("StorageUtil", "ClearFormListPrefix")
                .unwrap()
                .operation,
            "storage.prefix-clear"
        );
        assert_eq!(
            source_alias("StorageUtil", "FormListCopy")
                .unwrap()
                .operation,
            "storage.array-get+queue-replace"
        );
        assert_eq!(
            source_alias("StorageUtil", "IntListSlice")
                .unwrap()
                .value_kind,
            "none"
        );
        assert_eq!(
            source_alias("StorageUtil", "FormListFilterByTypes")
                .unwrap()
                .operation,
            "storage.array-get+form-type-filter"
        );
        assert_eq!(
            classify_static_call("StorageUtil", "GetFloatValue")
                .unwrap()
                .disposition,
            CompatibilityDisposition::Native
        );
        let declarations = papyrus_storage_util_declarations();
        assert_eq!(declarations.len(), 124);
        assert!(declarations
            .iter()
            .all(|function| function.declaration.validate().is_ok()));
        let slice = declarations
            .iter()
            .find(|function| {
                function
                    .declaration
                    .papyrus
                    .as_ref()
                    .is_some_and(|alias| alias.function == "IntListSlice")
            })
            .expect("IntListSlice declaration");
        assert_eq!(slice.declaration.result, None);
        assert_eq!(slice.declaration.parameters.len(), 4);
    }

    #[test]
    fn jcontainers_aliases_only_claim_the_executable_core_surface() {
        let create = source_alias("jarray", "OBJECT").unwrap();
        assert_eq!(create.service, LEGACY_CONTAINERS_SERVICE);
        assert_eq!(create.operation, "legacy-containers.array-create");
        let nested = source_alias("JMap", "setObj").unwrap();
        assert_eq!(nested.operation, "legacy-containers.map-set");
        assert_eq!(nested.value_kind, "handle");
        assert_eq!(
            classify_static_call("JArray", "getForm")
                .unwrap()
                .disposition,
            CompatibilityDisposition::Mapped
        );
        assert!(source_alias("JDB", "solveObj").is_none());
        assert_eq!(
            classify_static_call("JDB", "solveObj").unwrap().disposition,
            CompatibilityDisposition::Unsupported
        );
        assert_eq!(
            classify_static_call("JArray", "writeToFile")
                .unwrap()
                .disposition,
            CompatibilityDisposition::Unsupported
        );
        let declarations = papyrus_legacy_container_declarations();
        assert_eq!(declarations.len(), 46);
        assert!(declarations
            .iter()
            .all(|function| function.declaration.validate().is_ok()));
        let release =
            declarations
                .iter()
                .find(|function| {
                    function.declaration.papyrus.as_ref().is_some_and(|alias| {
                        alias.provider == "JValue" && alias.function == "release"
                    })
                })
                .unwrap();
        assert_eq!(
            release.declaration.result,
            Some(ScriptResultDeclaration {
                value_type: ScriptValueType::Integer,
                optional: false,
            })
        );
        let retain = source_alias("JValue", "retain").unwrap();
        assert_eq!(retain.operation, "legacy-containers.retain");
        let release_tagged = source_alias("JValue", "releaseObjectsWithTag").unwrap();
        assert_eq!(
            release_tagged.operation,
            "legacy-containers.release-objects-with-tag"
        );
    }

    #[test]
    fn storage_util_adapter_preserves_scalar_return_and_delete_contracts() {
        let set = adapt_storage_util_global_scalar(
            "MyMod.Score",
            StorageUtilScalarCall::SetInt { value: 12 },
            None,
        )
        .unwrap();
        assert_eq!(set.key.as_str(), "storageutil.int:mymod.score");
        assert_eq!(set.result, StorageUtilScalarResult::Int(12));
        assert_eq!(
            set.command,
            Some(PrincipalStorageCommand::Set {
                key: set.key.clone(),
                value: ExtensionValue::I64(12),
            })
        );

        let zero = adapt_storage_util_global_scalar(
            "MYMOD.SCORE",
            StorageUtilScalarCall::SetInt { value: 0 },
            Some(&PrincipalStorageValue::I64(12)),
        )
        .unwrap();
        assert_eq!(zero.key, set.key);
        assert!(matches!(
            zero.command,
            Some(PrincipalStorageCommand::Delete { .. })
        ));

        let unset = adapt_storage_util_global_scalar(
            "MyMod.Name",
            StorageUtilScalarCall::UnsetString,
            Some(&PrincipalStorageValue::String("Dragonborn".to_owned())),
        )
        .unwrap();
        assert_eq!(unset.result, StorageUtilScalarResult::Bool(true));
        assert!(matches!(
            unset.command,
            Some(PrincipalStorageCommand::Delete { .. })
        ));
    }

    #[test]
    fn storage_util_adapter_type_isolates_keys_and_honors_missing_values() {
        let get_int = adapt_storage_util_global_scalar(
            "SharedKey",
            StorageUtilScalarCall::GetInt { missing: 7 },
            None,
        )
        .unwrap();
        let get_string = adapt_storage_util_global_scalar(
            "sharedkey",
            StorageUtilScalarCall::GetString {
                missing: "fallback".to_owned(),
            },
            None,
        )
        .unwrap();
        assert_ne!(get_int.key, get_string.key);
        assert_eq!(get_int.result, StorageUtilScalarResult::Int(7));
        assert_eq!(
            get_string.result,
            StorageUtilScalarResult::String("fallback".to_owned())
        );
        assert!(get_int.command.is_none());
        assert!(get_string.command.is_none());
    }

    #[test]
    fn storage_util_adapter_round_trips_float_form_and_numeric_adjustments() {
        let float = adapt_storage_util_global_scalar(
            "SharedKey",
            StorageUtilScalarCall::SetFloat { value: 1.25 },
            None,
        )
        .unwrap();
        assert_eq!(float.key.as_str(), "storageutil.float:sharedkey");
        let Some(PrincipalStorageCommand::Set {
            value: ExtensionValue::Bytes(encoded),
            ..
        }) = float.command
        else {
            panic!("float set must encode a bounded byte value");
        };
        let adjusted = adapt_storage_util_global_scalar(
            "sharedkey",
            StorageUtilScalarCall::AdjustFloat { amount: 0.5 },
            Some(&PrincipalStorageValue::Bytes(encoded)),
        )
        .unwrap();
        assert_eq!(adjusted.result, StorageUtilScalarResult::Float(1.75));

        let adjusted_int = adapt_storage_util_global_scalar(
            "new-count",
            StorageUtilScalarCall::AdjustInt { amount: 4 },
            None,
        )
        .unwrap();
        assert_eq!(adjusted_int.result, StorageUtilScalarResult::Int(4));

        let form = FormRef::new([0x2a; 16], 0x1234_5678);
        let set_form = adapt_storage_util_global_scalar(
            "SharedKey",
            StorageUtilScalarCall::SetForm { value: Some(form) },
            None,
        )
        .unwrap();
        assert_eq!(set_form.key.as_str(), "storageutil.form:sharedkey");
        let Some(PrincipalStorageCommand::Set {
            value: ExtensionValue::Bytes(encoded),
            ..
        }) = set_form.command
        else {
            panic!("form set must encode a bounded byte value");
        };
        assert_eq!(encoded.len(), 20);
        assert_eq!(
            adapt_storage_util_global_scalar(
                "sharedkey",
                StorageUtilScalarCall::GetForm { missing: None },
                Some(&PrincipalStorageValue::Bytes(encoded)),
            )
            .unwrap()
            .result,
            StorageUtilScalarResult::Form(Some(form))
        );
        assert_ne!(float.key, set_form.key);
    }

    #[test]
    fn storage_util_adapter_plucks_each_scalar_type_and_deletes_missing_keys() {
        let int = adapt_storage_util_global_scalar(
            "count",
            StorageUtilScalarCall::PluckInt { missing: -1 },
            Some(&PrincipalStorageValue::I64(9)),
        )
        .unwrap();
        assert_eq!(int.result, StorageUtilScalarResult::Int(9));
        assert!(matches!(
            int.command,
            Some(PrincipalStorageCommand::Delete { .. })
        ));

        let float = adapt_storage_util_global_scalar(
            "ratio",
            StorageUtilScalarCall::PluckFloat { missing: -1.0 },
            Some(&PrincipalStorageValue::Bytes(
                2.5_f32.to_bits().to_le_bytes().to_vec(),
            )),
        )
        .unwrap();
        assert_eq!(float.result, StorageUtilScalarResult::Float(2.5));

        let string = adapt_storage_util_global_scalar(
            "status",
            StorageUtilScalarCall::PluckString {
                missing: "missing".to_owned(),
            },
            None,
        )
        .unwrap();
        assert_eq!(
            string.result,
            StorageUtilScalarResult::String("missing".to_owned())
        );
        assert!(matches!(
            string.command,
            Some(PrincipalStorageCommand::Delete { .. })
        ));

        let form = FormRef::new([0x17; 16], 0x42);
        let plucked_form = adapt_storage_util_global_scalar(
            "owner",
            StorageUtilScalarCall::PluckForm { missing: None },
            Some(&PrincipalStorageValue::Bytes(encode_storage_util_form(
                form,
            ))),
        )
        .unwrap();
        assert_eq!(
            plucked_form.result,
            StorageUtilScalarResult::Form(Some(form))
        );
    }

    #[test]
    fn storage_util_list_adapter_is_typed_bounded_and_preserves_legacy_results() {
        let add = adapt_storage_util_global_list(
            "Recent",
            StorageUtilListKind::Int,
            StorageUtilListCall::Add {
                value: StorageUtilListValue::Int(4),
                allow_duplicate: true,
            },
            None,
            4,
        )
        .unwrap();
        assert_eq!(add.key.as_str(), "storageutil.list.int:recent");
        assert_eq!(add.result, StorageUtilListResult::Int(0));
        assert_eq!(
            add.commands,
            [PrincipalStorageCommand::ArrayPush {
                key: add.key.clone(),
                value: ExtensionValue::I64(4),
            }]
        );

        let current = PrincipalStorageValue::Array(vec![ExtensionValue::I64(4)]);
        let duplicate = adapt_storage_util_global_list(
            "recent",
            StorageUtilListKind::Int,
            StorageUtilListCall::Add {
                value: StorageUtilListValue::Int(4),
                allow_duplicate: false,
            },
            Some(&current),
            4,
        )
        .unwrap();
        assert_eq!(duplicate.result, StorageUtilListResult::Int(-1));
        assert!(duplicate.commands.is_empty());
        assert_eq!(
            adapt_storage_util_global_list(
                "recent",
                StorageUtilListKind::Int,
                StorageUtilListCall::Find {
                    value: StorageUtilListValue::Int(4),
                },
                Some(&current),
                4,
            )
            .unwrap()
            .result,
            StorageUtilListResult::Int(0)
        );
        assert_eq!(
            adapt_storage_util_global_list(
                "recent",
                StorageUtilListKind::Int,
                StorageUtilListCall::Get { index: -1 },
                Some(&current),
                4,
            )
            .unwrap()
            .result,
            StorageUtilListResult::Value(StorageUtilListValue::Int(0))
        );

        let none_form = PrincipalStorageValue::Array(vec![ExtensionValue::Bytes(Vec::new())]);
        assert_eq!(
            adapt_storage_util_global_list(
                "owners",
                StorageUtilListKind::Form,
                StorageUtilListCall::Get { index: 0 },
                Some(&none_form),
                4,
            )
            .unwrap()
            .result,
            StorageUtilListResult::Value(StorageUtilListValue::Form(None))
        );

        assert_eq!(
            adapt_storage_util_global_list(
                "ratios",
                StorageUtilListKind::Float,
                StorageUtilListCall::Count,
                Some(&PrincipalStorageValue::Array(vec![ExtensionValue::Bytes(
                    vec![0; 3],
                )])),
                4,
            ),
            Err(StorageUtilAdapterError::TypeMismatch)
        );
    }

    #[test]
    fn storage_util_list_random_selects_a_member_and_defaults_when_empty() {
        let current = PrincipalStorageValue::Array(vec![
            ExtensionValue::String("zero".to_owned()),
            ExtensionValue::String("one".to_owned()),
            ExtensionValue::String("two".to_owned()),
        ]);
        let selected = adapt_storage_util_global_list(
            "labels",
            StorageUtilListKind::String,
            StorageUtilListCall::Random { selector: 7 },
            Some(&current),
            4,
        )
        .unwrap();
        assert_eq!(
            selected.result,
            StorageUtilListResult::Value(StorageUtilListValue::String("one".to_owned()))
        );
        assert!(selected.commands.is_empty());

        let empty = adapt_storage_util_global_list(
            "labels",
            StorageUtilListKind::String,
            StorageUtilListCall::Random { selector: 7 },
            None,
            4,
        )
        .unwrap();
        assert_eq!(
            empty.result,
            StorageUtilListResult::Value(StorageUtilListValue::String(String::new()))
        );
        assert!(empty.commands.is_empty());
    }

    #[test]
    fn storage_util_list_copy_and_to_array_preserve_typed_values_and_bounds() {
        let replacement = vec![StorageUtilListValue::Int(3), StorageUtilListValue::Int(5)];
        let copied = adapt_storage_util_global_list(
            "numbers",
            StorageUtilListKind::Int,
            StorageUtilListCall::Copy {
                values: replacement.clone(),
            },
            None,
            4,
        )
        .unwrap();
        assert_eq!(copied.result, StorageUtilListResult::Bool(true));
        assert_eq!(
            copied.commands,
            [PrincipalStorageCommand::ArrayReplace {
                key: copied.key.clone(),
                values: vec![ExtensionValue::I64(3), ExtensionValue::I64(5)],
            }]
        );

        let current =
            PrincipalStorageValue::Array(vec![ExtensionValue::I64(3), ExtensionValue::I64(5)]);
        let array = adapt_storage_util_global_list(
            "numbers",
            StorageUtilListKind::Int,
            StorageUtilListCall::ToArray,
            Some(&current),
            4,
        )
        .unwrap();
        assert_eq!(array.result, StorageUtilListResult::Array(replacement));
        assert!(array.commands.is_empty());

        let too_large = adapt_storage_util_global_list(
            "numbers",
            StorageUtilListKind::Int,
            StorageUtilListCall::Copy {
                values: vec![StorageUtilListValue::Int(1); 5],
            },
            Some(&current),
            4,
        )
        .unwrap();
        assert_eq!(too_large.result, StorageUtilListResult::Bool(false));
        assert!(too_large.commands.is_empty());
    }

    #[test]
    fn storage_util_list_slice_fills_existing_typed_array_without_storage_mutation() {
        let current = PrincipalStorageValue::Array(vec![
            ExtensionValue::I64(1),
            ExtensionValue::I64(2),
            ExtensionValue::I64(6),
            ExtensionValue::I64(9),
        ]);
        let slice = adapt_storage_util_global_list(
            "numbers",
            StorageUtilListKind::Int,
            StorageUtilListCall::Slice {
                values: vec![StorageUtilListValue::Int(0); 2],
                start_index: 1,
            },
            Some(&current),
            4,
        )
        .unwrap();
        assert_eq!(
            slice.result,
            StorageUtilListResult::Array(vec![
                StorageUtilListValue::Int(2),
                StorageUtilListValue::Int(6),
            ])
        );
        assert!(slice.commands.is_empty());

        let negative = adapt_storage_util_global_list(
            "numbers",
            StorageUtilListKind::Int,
            StorageUtilListCall::Slice {
                values: vec![StorageUtilListValue::Int(7); 2],
                start_index: -1,
            },
            Some(&current),
            4,
        )
        .unwrap();
        assert_eq!(
            negative.result,
            StorageUtilListResult::Array(vec![
                StorageUtilListValue::Int(7),
                StorageUtilListValue::Int(7),
            ])
        );
        assert!(negative.commands.is_empty());
    }

    #[test]
    fn storage_util_form_filter_uses_cataloged_record_types_and_omits_unknowns() {
        let source = 1_u128.to_be_bytes();
        let weapon = FormRef::new(source, 0x1234);
        let armor = FormRef::new(source, 0x1235);
        let catalog = ContentCatalog::new_with_metadata(
            vec![PluginInfo::new("Skyrim.esm", source, PluginKind::Regular).unwrap()],
            vec![vec![]],
            vec![vec![(0x1234, *b"WEAP"), (0x1235, *b"ARMO")]],
        )
        .unwrap();
        assert_eq!(storage_util_form_type_id(&catalog, weapon), Some(41));
        assert_eq!(storage_util_form_type_id(&catalog, armor), Some(26));
        let current = PrincipalStorageValue::Array(vec![
            ExtensionValue::Bytes(encode_storage_util_form(weapon)),
            ExtensionValue::Bytes(Vec::new()),
            ExtensionValue::Bytes(encode_storage_util_form(armor)),
        ]);

        let matching =
            adapt_storage_util_global_form_filter("owners", &[41], true, Some(&current), &catalog)
                .unwrap();
        assert_eq!(
            matching.result,
            StorageUtilListResult::Array(vec![StorageUtilListValue::Form(Some(weapon))])
        );
        assert!(matching.commands.is_empty());

        let inverse =
            adapt_storage_util_global_form_filter("owners", &[41], false, Some(&current), &catalog)
                .unwrap();
        assert_eq!(
            inverse.result,
            StorageUtilListResult::Array(vec![StorageUtilListValue::Form(Some(armor))])
        );
    }

    #[test]
    fn storage_util_prefix_operations_are_type_scoped_case_folded_and_atomic() {
        let values = BTreeMap::from([
            (
                StorageKey::new("storageutil.int:mod.score").unwrap(),
                PrincipalStorageValue::I64(3),
            ),
            (
                StorageKey::new("storageutil.int:other.score").unwrap(),
                PrincipalStorageValue::I64(4),
            ),
            (
                StorageKey::new("storageutil.list.string:mod.labels").unwrap(),
                PrincipalStorageValue::Array(vec![ExtensionValue::String("a".to_owned())]),
            ),
            (
                StorageKey::new("unrelated").unwrap(),
                PrincipalStorageValue::I64(5),
            ),
        ]);
        let typed = adapt_storage_util_global_prefix(
            "MOD.",
            StorageUtilPrefixKind::IntValue,
            StorageUtilPrefixOperation::Count,
            Some(&values),
        )
        .unwrap();
        assert_eq!(typed.result, 1);
        assert!(typed.commands.is_empty());

        let all = adapt_storage_util_global_prefix(
            "mod.",
            StorageUtilPrefixKind::All,
            StorageUtilPrefixOperation::Clear,
            Some(&values),
        )
        .unwrap();
        assert_eq!(all.result, 2);
        assert_eq!(all.commands.len(), 2);
        assert!(all.commands.iter().all(|command| matches!(
            command,
            PrincipalStorageCommand::Delete { key } if key.as_str().contains(":mod.")
        )));
        assert_eq!(
            adapt_storage_util_global_prefix(
                "",
                StorageUtilPrefixKind::All,
                StorageUtilPrefixOperation::Count,
                Some(&values),
            ),
            Err(StorageUtilAdapterError::EmptyPrefix)
        );
    }

    #[test]
    fn storage_util_list_mutations_return_previous_values_and_queue_exact_edits() {
        let current =
            PrincipalStorageValue::Array(vec![ExtensionValue::I64(4), ExtensionValue::I64(9)]);
        let set = adapt_storage_util_global_list(
            "recent",
            StorageUtilListKind::Int,
            StorageUtilListCall::Set {
                index: 1,
                value: StorageUtilListValue::Int(7),
            },
            Some(&current),
            4,
        )
        .unwrap();
        assert_eq!(
            set.result,
            StorageUtilListResult::Value(StorageUtilListValue::Int(9))
        );
        assert_eq!(
            set.commands,
            [PrincipalStorageCommand::ArraySet {
                key: set.key.clone(),
                index: 1,
                value: ExtensionValue::I64(7),
            }]
        );

        let pluck = adapt_storage_util_global_list(
            "recent",
            StorageUtilListKind::Int,
            StorageUtilListCall::Pluck {
                index: 0,
                missing: StorageUtilListValue::Int(-1),
            },
            Some(&current),
            4,
        )
        .unwrap();
        assert_eq!(
            pluck.result,
            StorageUtilListResult::Value(StorageUtilListValue::Int(4))
        );
        assert_eq!(
            pluck.commands,
            [PrincipalStorageCommand::ArrayRemove {
                key: pluck.key.clone(),
                index: 0,
            }]
        );

        let missing = adapt_storage_util_global_list(
            "recent",
            StorageUtilListKind::Int,
            StorageUtilListCall::Pluck {
                index: -1,
                missing: StorageUtilListValue::Int(-1),
            },
            Some(&current),
            4,
        )
        .unwrap();
        assert_eq!(
            missing.result,
            StorageUtilListResult::Value(StorageUtilListValue::Int(-1))
        );
        assert!(missing.commands.is_empty());

        for (call, expected, index) in [
            (StorageUtilListCall::Shift, 4, 0),
            (StorageUtilListCall::Pop, 9, 1),
        ] {
            let adapted = adapt_storage_util_global_list(
                "recent",
                StorageUtilListKind::Int,
                call,
                Some(&current),
                4,
            )
            .unwrap();
            assert_eq!(
                adapted.result,
                StorageUtilListResult::Value(StorageUtilListValue::Int(expected))
            );
            assert_eq!(
                adapted.commands,
                [PrincipalStorageCommand::ArrayRemove {
                    key: adapted.key.clone(),
                    index,
                }]
            );
        }

        let removed = adapt_storage_util_global_list(
            "recent",
            StorageUtilListKind::Int,
            StorageUtilListCall::RemoveAt { index: 1 },
            Some(&current),
            4,
        )
        .unwrap();
        assert_eq!(removed.result, StorageUtilListResult::Bool(true));
        assert_eq!(
            removed.commands,
            [PrincipalStorageCommand::ArrayRemove {
                key: removed.key.clone(),
                index: 1,
            }]
        );

        let not_removed = adapt_storage_util_global_list(
            "recent",
            StorageUtilListKind::Int,
            StorageUtilListCall::RemoveAt { index: 2 },
            Some(&current),
            4,
        )
        .unwrap();
        assert_eq!(not_removed.result, StorageUtilListResult::Bool(false));
        assert!(not_removed.commands.is_empty());
    }

    #[test]
    fn storage_util_list_value_mutations_are_atomic_bounded_and_exact() {
        let current = PrincipalStorageValue::Array(vec![
            ExtensionValue::I64(2),
            ExtensionValue::I64(4),
            ExtensionValue::I64(2),
        ]);
        let insert = adapt_storage_util_global_list(
            "numbers",
            StorageUtilListKind::Int,
            StorageUtilListCall::Insert {
                index: 1,
                value: StorageUtilListValue::Int(3),
            },
            Some(&current),
            4,
        )
        .unwrap();
        assert_eq!(insert.result, StorageUtilListResult::Bool(true));
        assert_eq!(
            insert.commands,
            [PrincipalStorageCommand::ArrayReplace {
                key: insert.key.clone(),
                values: vec![
                    ExtensionValue::I64(2),
                    ExtensionValue::I64(3),
                    ExtensionValue::I64(4),
                    ExtensionValue::I64(2),
                ],
            }]
        );

        let full = adapt_storage_util_global_list(
            "numbers",
            StorageUtilListKind::Int,
            StorageUtilListCall::Insert {
                index: 3,
                value: StorageUtilListValue::Int(5),
            },
            Some(&current),
            3,
        )
        .unwrap();
        assert_eq!(full.result, StorageUtilListResult::Bool(false));
        assert!(full.commands.is_empty());

        for (all_instances, expected_count, expected_values) in [
            (
                false,
                1,
                vec![ExtensionValue::I64(4), ExtensionValue::I64(2)],
            ),
            (true, 2, vec![ExtensionValue::I64(4)]),
        ] {
            let remove = adapt_storage_util_global_list(
                "numbers",
                StorageUtilListKind::Int,
                StorageUtilListCall::Remove {
                    value: StorageUtilListValue::Int(2),
                    all_instances,
                },
                Some(&current),
                4,
            )
            .unwrap();
            assert_eq!(remove.result, StorageUtilListResult::Int(expected_count));
            assert_eq!(
                remove.commands,
                [PrincipalStorageCommand::ArrayReplace {
                    key: remove.key.clone(),
                    values: expected_values,
                }]
            );
        }

        for (exclude, expected) in [(false, 2), (true, 1)] {
            assert_eq!(
                adapt_storage_util_global_list(
                    "numbers",
                    StorageUtilListKind::Int,
                    StorageUtilListCall::CountValue {
                        value: StorageUtilListValue::Int(2),
                        exclude,
                    },
                    Some(&current),
                    4,
                )
                .unwrap()
                .result,
                StorageUtilListResult::Int(expected)
            );
        }

        let adjust = adapt_storage_util_global_list(
            "numbers",
            StorageUtilListKind::Int,
            StorageUtilListCall::Adjust {
                index: 1,
                amount: StorageUtilListValue::Int(3),
            },
            Some(&current),
            4,
        )
        .unwrap();
        assert_eq!(
            adjust.result,
            StorageUtilListResult::Value(StorageUtilListValue::Int(7))
        );
        assert_eq!(
            adjust.commands,
            [PrincipalStorageCommand::ArraySet {
                key: adjust.key.clone(),
                index: 1,
                value: ExtensionValue::I64(7),
            }]
        );

        let missing = adapt_storage_util_global_list(
            "numbers",
            StorageUtilListKind::Int,
            StorageUtilListCall::Adjust {
                index: -1,
                amount: StorageUtilListValue::Int(3),
            },
            Some(&current),
            4,
        )
        .unwrap();
        assert_eq!(
            missing.result,
            StorageUtilListResult::Value(StorageUtilListValue::Int(0))
        );
        assert!(missing.commands.is_empty());

        let float = adapt_storage_util_global_list(
            "ratios",
            StorageUtilListKind::Float,
            StorageUtilListCall::Adjust {
                index: 0,
                amount: StorageUtilListValue::Float(0.5),
            },
            Some(&PrincipalStorageValue::Array(vec![ExtensionValue::Bytes(
                1.5_f32.to_bits().to_le_bytes().to_vec(),
            )])),
            4,
        )
        .unwrap();
        assert_eq!(
            float.result,
            StorageUtilListResult::Value(StorageUtilListValue::Float(2.0))
        );
    }

    #[test]
    fn storage_util_list_sort_and_resize_preserve_upstream_bounds_and_deltas() {
        let current = PrincipalStorageValue::Array(vec![
            ExtensionValue::I64(3),
            ExtensionValue::I64(1),
            ExtensionValue::I64(2),
        ]);
        let sorted = adapt_storage_util_global_list(
            "numbers",
            StorageUtilListKind::Int,
            StorageUtilListCall::Sort,
            Some(&current),
            600,
        )
        .unwrap();
        assert_eq!(sorted.result, StorageUtilListResult::None);
        assert_eq!(
            sorted.commands,
            [PrincipalStorageCommand::ArrayReplace {
                key: sorted.key.clone(),
                values: vec![
                    ExtensionValue::I64(1),
                    ExtensionValue::I64(2),
                    ExtensionValue::I64(3),
                ],
            }]
        );

        let grown = adapt_storage_util_global_list(
            "numbers",
            StorageUtilListKind::Int,
            StorageUtilListCall::Resize {
                to_length: 5,
                filler: StorageUtilListValue::Int(9),
            },
            Some(&current),
            600,
        )
        .unwrap();
        assert_eq!(grown.result, StorageUtilListResult::Int(2));
        assert_eq!(
            grown.commands,
            [PrincipalStorageCommand::ArrayReplace {
                key: grown.key.clone(),
                values: vec![
                    ExtensionValue::I64(3),
                    ExtensionValue::I64(1),
                    ExtensionValue::I64(2),
                    ExtensionValue::I64(9),
                    ExtensionValue::I64(9),
                ],
            }]
        );

        let shrunk = adapt_storage_util_global_list(
            "numbers",
            StorageUtilListKind::Int,
            StorageUtilListCall::Resize {
                to_length: 1,
                filler: StorageUtilListValue::Int(0),
            },
            Some(&current),
            600,
        )
        .unwrap();
        assert_eq!(shrunk.result, StorageUtilListResult::Int(-2));
        assert_eq!(
            shrunk.commands,
            [PrincipalStorageCommand::ArrayReplace {
                key: shrunk.key.clone(),
                values: vec![ExtensionValue::I64(3)],
            }]
        );

        let cleared = adapt_storage_util_global_list(
            "numbers",
            StorageUtilListKind::Int,
            StorageUtilListCall::Resize {
                to_length: 0,
                filler: StorageUtilListValue::Int(0),
            },
            Some(&current),
            600,
        )
        .unwrap();
        assert_eq!(cleared.result, StorageUtilListResult::Int(-3));
        assert_eq!(
            cleared.commands,
            [PrincipalStorageCommand::Delete {
                key: cleared.key.clone(),
            }]
        );

        for invalid_length in [-1, 501] {
            let invalid = adapt_storage_util_global_list(
                "numbers",
                StorageUtilListKind::Int,
                StorageUtilListCall::Resize {
                    to_length: invalid_length,
                    filler: StorageUtilListValue::Int(0),
                },
                Some(&current),
                600,
            )
            .unwrap();
            assert_eq!(invalid.result, StorageUtilListResult::Int(0));
            assert!(invalid.commands.is_empty());
        }

        let first = FormRef::new([1; 16], 1);
        let second = FormRef::new([1; 16], 2);
        let forms = PrincipalStorageValue::Array(vec![
            ExtensionValue::Bytes(encode_storage_util_form(second)),
            ExtensionValue::Bytes(Vec::new()),
            ExtensionValue::Bytes(encode_storage_util_form(first)),
        ]);
        let sorted_forms = adapt_storage_util_global_list(
            "owners",
            StorageUtilListKind::Form,
            StorageUtilListCall::Sort,
            Some(&forms),
            600,
        )
        .unwrap();
        assert_eq!(
            sorted_forms.commands,
            [PrincipalStorageCommand::ArrayReplace {
                key: sorted_forms.key.clone(),
                values: vec![
                    ExtensionValue::Bytes(Vec::new()),
                    ExtensionValue::Bytes(encode_storage_util_form(first)),
                    ExtensionValue::Bytes(encode_storage_util_form(second)),
                ],
            }]
        );
    }

    #[test]
    fn storage_util_adapter_rejects_unrepresentable_or_corrupt_values() {
        assert!(matches!(
            adapt_storage_util_global_scalar(
                "contains spaces",
                StorageUtilScalarCall::HasInt,
                None,
            ),
            Err(StorageUtilAdapterError::InvalidKey(_))
        ));
        assert_eq!(
            adapt_storage_util_global_scalar(
                "score",
                StorageUtilScalarCall::GetInt { missing: 0 },
                Some(&PrincipalStorageValue::String("wrong".to_owned())),
            ),
            Err(StorageUtilAdapterError::TypeMismatch)
        );
        assert_eq!(
            adapt_storage_util_global_scalar(
                "score",
                StorageUtilScalarCall::AdjustInt { amount: 1 },
                Some(&PrincipalStorageValue::I64(i64::from(i32::MAX))),
            ),
            Err(StorageUtilAdapterError::IntegerOverflow)
        );
        assert_eq!(
            adapt_storage_util_global_scalar(
                "ratio",
                StorageUtilScalarCall::SetFloat { value: f32::NAN },
                None,
            ),
            Err(StorageUtilAdapterError::NonFiniteFloat)
        );
        assert_eq!(
            adapt_storage_util_global_scalar(
                "ratio",
                StorageUtilScalarCall::GetFloat { missing: f32::NAN },
                None,
            ),
            Err(StorageUtilAdapterError::NonFiniteFloat)
        );
        assert_eq!(
            adapt_storage_util_global_scalar(
                "owner",
                StorageUtilScalarCall::HasForm,
                Some(&PrincipalStorageValue::Bytes(vec![0; 19])),
            ),
            Err(StorageUtilAdapterError::TypeMismatch)
        );
    }

    #[test]
    fn fixed_mod_event_adapter_preserves_name_payload_and_sender() {
        let sender = FormRef::new([0x2a; 16], 0x800);
        let command = adapt_legacy_send_mod_event(
            "SKICP_configManagerReady",
            "page:selected".to_owned(),
            42.5,
            Some(sender),
        )
        .unwrap();
        assert_eq!(
            crate::event::legacy_skse_mod_event_name(&command.event).as_deref(),
            Some("SKICP_configManagerReady")
        );
        let payload = LegacySkseModEventPayload::decode(&command.payload).unwrap();
        assert_eq!(payload.string_arg, "page:selected");
        assert_eq!(payload.number_arg(), 42.5);
        assert_eq!(payload.sender, Some(sender));
        assert_eq!(
            method_source_alias("sendmodevent").unwrap().operation,
            "events.publish"
        );
        assert_eq!(
            method_source_alias("RegisterForModEvent")
                .unwrap()
                .operation,
            "events.queue-legacy-subscribe"
        );
        assert_eq!(
            method_source_alias("UnregisterForAllModEvents")
                .unwrap()
                .operation,
            "events.queue-legacy-unsubscribe-all"
        );
    }

    #[test]
    fn mod_event_catalog_does_not_map_unknown_provider_functions() {
        assert_eq!(
            classify_static_call("ModEvent", "UnknownHandleOperation")
                .unwrap()
                .disposition,
            CompatibilityDisposition::Unsupported
        );
        assert_eq!(
            classify_static_call("ModEvent", "PushString")
                .unwrap()
                .disposition,
            CompatibilityDisposition::Native
        );
        assert_eq!(
            source_alias("ModEvent", "Create").unwrap().operation,
            "events.legacy-builder-create"
        );
        assert_eq!(
            source_alias("ModEvent", "PushForm").unwrap().value_kind,
            "form"
        );
        let declarations = papyrus_mod_event_declarations();
        assert_eq!(declarations.len(), 8);
        for declaration in declarations {
            declaration.declaration.validate().unwrap();
        }
    }

    #[test]
    fn unsafe_host_facilities_are_explicitly_unsupported() {
        let json = classify_static_call("JsonUtil", "Load").unwrap();
        assert_eq!(json.disposition, CompatibilityDisposition::Unsupported);
        assert!(json.guidance.contains("sandbox"));
        let input = classify_static_call("Input", "TapKey").unwrap();
        assert_eq!(input.disposition, CompatibilityDisposition::Unsupported);
        assert!(input.guidance.contains("normalized"));
    }

    #[test]
    fn vanilla_and_unknown_mod_calls_are_not_misclassified() {
        assert!(classify_static_call("Utility", "Wait").is_none());
        assert!(classify_method_call("MyModFunction").is_none());
        assert_eq!(
            classify_static_call("SKSE", "UnknownNative")
                .unwrap()
                .disposition,
            CompatibilityDisposition::Unsupported
        );
    }
}
