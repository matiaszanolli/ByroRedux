//! Layer 3 — classifiers mapping a legacy call to its `SourceAlias`.

use super::*;

/// Exact source recipe for load-order discovery commands shared by OBSE and
/// xNVSE. Numeric indices are callback-local compatibility values; portable
/// authored identity remains [`FormRef`].
pub fn obscript_source_alias(command: &str) -> Option<SourceAlias> {
    let (function, operation, value_kind, constraint) =
        if command.eq_ignore_ascii_case("IsModLoaded") {
            (
                "IsModLoaded",
                "content.find",
                "bool",
                "plugin basename is case-insensitive and includes its extension",
            )
        } else if command.eq_ignore_ascii_case("GetModIndex") {
            (
                "GetModIndex",
                "content.find-index",
                "signed",
                "index is callback-local; missing xNVSE plugins use the legacy 255 sentinel",
            )
        } else if command.eq_ignore_ascii_case("GetNumLoadedMods") {
            (
                "GetNumLoadedMods",
                "content.count",
                "signed",
                "legacy xNVSE name; the catalog order is immutable for the callback",
            )
        } else if command.eq_ignore_ascii_case("GetNumLoadedPlugins") {
            (
                "GetNumLoadedPlugins",
                "content.count",
                "signed",
                "legacy OBSE name; the catalog order is immutable for the callback",
            )
        } else if command.eq_ignore_ascii_case("GetNthModName") {
            (
                "GetNthModName",
                "content.plugin-name",
                "text",
                "index is callback-local; use stable source identity outside the adapter",
            )
        } else {
            return None;
        };
    Some(SourceAlias {
        provider: "xNVSE/OBSE",
        function,
        service: CONTENT_CATALOG_SERVICE,
        operation,
        value_kind,
        constraint,
    })
}

/// Classify extender commands embedded in Oblivion/FO3/FNV ObScript source.
/// Version probes map to semantic feature discovery rather than pretending an
/// external loader is installed.
pub fn classify_obscript_command(command: &str) -> Option<CompatibilityMatch> {
    if obscript_source_alias(command).is_some() {
        return Some(native(
            ExtenderFamily::Shared,
            CONTENT_CATALOG_SERVICE,
            "an exact engine adapter executes the legacy load-order query against the immutable content catalog",
        ));
    }
    if command.eq_ignore_ascii_case("GetNVSEVersion")
        || command.eq_ignore_ascii_case("GetNVSERevision")
        || command.eq_ignore_ascii_case("GetNVSEBeta")
    {
        return Some(mapped(
            ExtenderFamily::Xnvse,
            CONTEXT_SERVICE,
            "replace xNVSE version gates with SDK/service feature discovery",
        ));
    }
    if command
        .get(.."GetNVSE".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("GetNVSE"))
    {
        return Some(unsupported(
            ExtenderFamily::Xnvse,
            "the xNVSE command namespace is recognized, but this command has no engine semantic mapping",
        ));
    }
    if command.eq_ignore_ascii_case("GetOBSEVersion")
        || command.eq_ignore_ascii_case("GetOBSERevision")
    {
        return Some(mapped(
            ExtenderFamily::Obse,
            CONTEXT_SERVICE,
            "replace OBSE version gates with SDK/service feature discovery",
        ));
    }
    if command
        .get(.."GetOBSE".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("GetOBSE"))
    {
        return Some(unsupported(
            ExtenderFamily::Obse,
            "the OBSE command namespace is recognized, but this command has no engine semantic mapping",
        ));
    }
    None
}

/// Exact source alias for extender-added instance calls.
pub fn method_source_alias(function: &str) -> Option<SourceAlias> {
    let (function, operation, value_kind, constraint) =
        if function.eq_ignore_ascii_case("SendModEvent") {
            (
                "SendModEvent",
                "events.publish",
                "legacy-skse-fixed-event",
                "event name <=53 UTF-8 bytes; sender must have a stable FormRef",
            )
        } else if function.eq_ignore_ascii_case("RegisterForModEvent") {
            (
                "RegisterForModEvent",
                "events.queue-legacy-subscribe",
                "legacy-skse-registration",
                "event name <=53 UTF-8 bytes; callback <=128 bytes; refresh after load",
            )
        } else if function.eq_ignore_ascii_case("UnregisterForModEvent") {
            (
                "UnregisterForModEvent",
                "events.queue-legacy-unsubscribe",
                "legacy-skse-registration",
                "event name <=53 UTF-8 bytes",
            )
        } else if function.eq_ignore_ascii_case("UnregisterForAllModEvents") {
            (
                "UnregisterForAllModEvents",
                "events.queue-legacy-unsubscribe-all",
                "legacy-skse-registration",
                "removes only runtime SKSE-compatibility registrations",
            )
        } else {
            return None;
        };
    Some(SourceAlias {
        provider: "Form/Alias/ActiveMagicEffect",
        function,
        service: EVENT_SERVICE,
        operation,
        value_kind,
        constraint,
    })
}

/// Resolve engine-backed PapyrusUtil source aliases.
///
/// `StorageUtil` namespaces values by `(object, key, type)`, while
/// `byro.storage` is private to one extension principal and keys each value
/// once. These aliases therefore cover only global (`ObjKey == None`) values
/// whose names fit the portable storage-key grammar. The
/// executable adapter below case-folds and type-namespaces those keys. Its
/// writes preserve synchronous return and same-callback visibility through the
/// host transaction overlay. Object-scoped values require extension
/// components, file access, and cross-principal sharing remain unsupported
/// until an engine service can honor their full contracts.
pub fn source_alias(provider: &str, function: &str) -> Option<SourceAlias> {
    if let Some(alias) = legacy_container_source_alias(provider, function) {
        return Some(alias);
    }
    if provider.eq_ignore_ascii_case("ModEvent") {
        let (function, operation, value_kind) = if function.eq_ignore_ascii_case("Create") {
            ("Create", "events.legacy-builder-create", "handle")
        } else if function.eq_ignore_ascii_case("Send") {
            ("Send", "events.legacy-builder-send", "bool")
        } else if function.eq_ignore_ascii_case("Release") {
            ("Release", "events.legacy-builder-release", "none")
        } else if function.eq_ignore_ascii_case("PushBool") {
            ("PushBool", "events.legacy-builder-push", "bool")
        } else if function.eq_ignore_ascii_case("PushInt") {
            ("PushInt", "events.legacy-builder-push", "signed")
        } else if function.eq_ignore_ascii_case("PushFloat") {
            ("PushFloat", "events.legacy-builder-push", "float")
        } else if function.eq_ignore_ascii_case("PushString") {
            ("PushString", "events.legacy-builder-push", "text")
        } else if function.eq_ignore_ascii_case("PushForm") {
            ("PushForm", "events.legacy-builder-push", "form")
        } else {
            return None;
        };
        return Some(SourceAlias {
            provider: "ModEvent",
            function,
            service: EVENT_SERVICE,
            operation,
            value_kind,
            constraint: "<=64 live handles; <=128 arguments; encoded payload <=4096 bytes",
        });
    }
    if provider.eq_ignore_ascii_case("Input") {
        let (function, operation, value_kind, constraint) = if function
            .eq_ignore_ascii_case("GetMappedKey")
        {
            (
                "GetMappedKey",
                "input.binding-key",
                "signed",
                "control matching is case-insensitive; device type is keyboard (0), mouse (1), gamepad (2), or auto (0xff)",
            )
        } else if function.eq_ignore_ascii_case("GetMappedControl") {
            (
                "GetMappedControl",
                "input.binding-control",
                "text",
                "returns an empty string for an unbound or unsupported key code",
            )
        } else {
            return None;
        };
        return Some(SourceAlias {
            provider: "Input",
            function,
            service: INPUT_SERVICE,
            operation,
            value_kind,
            constraint,
        });
    }
    if provider.eq_ignore_ascii_case("UI") {
        if function.eq_ignore_ascii_case("IsMenuOpen") {
            return Some(SourceAlias {
                provider: "UI",
                function: "IsMenuOpen",
                service: UI_SERVICE,
                operation: "ui.menu-is-open",
                value_kind: "bool",
                constraint:
                    "matches the active visible menu name exactly; empty names are never open",
            });
        }
        return None;
    }
    if !provider.eq_ignore_ascii_case("StorageUtil") {
        return None;
    }
    if let Some(alias) = storage_util_list_source_alias(function) {
        return Some(alias);
    }
    if let Some(alias) = storage_util_prefix_source_alias(function) {
        return Some(alias);
    }
    let (function, operation, value_kind) = if function.eq_ignore_ascii_case("GetIntValue") {
        ("GetIntValue", "storage.get", "signed")
    } else if function.eq_ignore_ascii_case("PluckIntValue") {
        ("PluckIntValue", "storage.get+queue-delete", "signed")
    } else if function.eq_ignore_ascii_case("HasIntValue") {
        ("HasIntValue", "storage.get", "signed")
    } else if function.eq_ignore_ascii_case("SetIntValue") {
        ("SetIntValue", "storage.queue-set/delete", "signed")
    } else if function.eq_ignore_ascii_case("UnsetIntValue") {
        ("UnsetIntValue", "storage.get+queue-delete", "signed")
    } else if function.eq_ignore_ascii_case("AdjustIntValue") {
        ("AdjustIntValue", "storage.get+queue-set/delete", "signed")
    } else if function.eq_ignore_ascii_case("GetFloatValue") {
        ("GetFloatValue", "storage.get", "float")
    } else if function.eq_ignore_ascii_case("PluckFloatValue") {
        ("PluckFloatValue", "storage.get+queue-delete", "float")
    } else if function.eq_ignore_ascii_case("HasFloatValue") {
        ("HasFloatValue", "storage.get", "float")
    } else if function.eq_ignore_ascii_case("SetFloatValue") {
        ("SetFloatValue", "storage.queue-set/delete", "float")
    } else if function.eq_ignore_ascii_case("UnsetFloatValue") {
        ("UnsetFloatValue", "storage.get+queue-delete", "float")
    } else if function.eq_ignore_ascii_case("AdjustFloatValue") {
        ("AdjustFloatValue", "storage.get+queue-set/delete", "float")
    } else if function.eq_ignore_ascii_case("GetStringValue") {
        ("GetStringValue", "storage.get", "text")
    } else if function.eq_ignore_ascii_case("PluckStringValue") {
        ("PluckStringValue", "storage.get+queue-delete", "text")
    } else if function.eq_ignore_ascii_case("HasStringValue") {
        ("HasStringValue", "storage.get", "text")
    } else if function.eq_ignore_ascii_case("SetStringValue") {
        ("SetStringValue", "storage.queue-set/delete", "text")
    } else if function.eq_ignore_ascii_case("UnsetStringValue") {
        ("UnsetStringValue", "storage.get+queue-delete", "text")
    } else if function.eq_ignore_ascii_case("GetFormValue") {
        ("GetFormValue", "storage.get", "form")
    } else if function.eq_ignore_ascii_case("PluckFormValue") {
        ("PluckFormValue", "storage.get+queue-delete", "form")
    } else if function.eq_ignore_ascii_case("HasFormValue") {
        ("HasFormValue", "storage.get", "form")
    } else if function.eq_ignore_ascii_case("SetFormValue") {
        ("SetFormValue", "storage.queue-set/delete", "form")
    } else if function.eq_ignore_ascii_case("UnsetFormValue") {
        ("UnsetFormValue", "storage.get+queue-delete", "form")
    } else {
        return None;
    };
    Some(SourceAlias {
        provider: "StorageUtil",
        function,
        service: PRINCIPAL_STORAGE_SERVICE,
        operation,
        value_kind,
        constraint: "ObjKey must be None; portable key; principal-private (no cross-mod sharing)",
    })
}

/// Classify a static Papyrus call by provider type and function name.
/// Returns `None` for providers that are not known extender APIs.
pub fn classify_static_call(provider: &str, function: &str) -> Option<CompatibilityMatch> {
    if provider.eq_ignore_ascii_case("Game")
        && matches_ignore_ascii_case(
            function,
            &[
                "GetPlayer",
                "GetModCount",
                "GetModByName",
                "GetFormFromFile",
                "GetModName",
                "GetModDependencyCount",
                "IsPluginInstalled",
                "GetLightModCount",
                "GetLightModByName",
                "GetLightModName",
                "GetLightModDependencyCount",
                "GetNthLightModDependency",
            ],
        )
    {
        return Some(native(
            ExtenderFamily::Skse,
            if function.eq_ignore_ascii_case("GetPlayer") {
                WORLD_PROJECTION_SERVICE
            } else {
                CONTENT_CATALOG_SERVICE
            },
            if function.eq_ignore_ascii_case("GetPlayer") {
                "executed by the engine player-identity bridge with a stable opaque entity handle"
            } else {
                "executed by the engine content catalog with exact regular/light index semantics"
            },
        ));
    }
    if provider.eq_ignore_ascii_case("SKSE") {
        return Some(if is_extender_version_probe(function) {
            mapped(
                ExtenderFamily::Skse,
                CONTEXT_SERVICE,
                "replace extender-version probes with SDK/service feature discovery",
            )
        } else {
            unsupported(
                ExtenderFamily::Skse,
                "the SKSE provider is recognized, but this function has no engine semantic mapping",
            )
        });
    }
    if provider.eq_ignore_ascii_case("F4SE") {
        return Some(if is_extender_version_probe(function) {
            mapped(
                ExtenderFamily::F4se,
                CONTEXT_SERVICE,
                "replace extender-version probes with SDK/service feature discovery",
            )
        } else {
            unsupported(
                ExtenderFamily::F4se,
                "the F4SE provider is recognized, but this function has no engine semantic mapping",
            )
        });
    }
    if provider.eq_ignore_ascii_case("StorageUtil") {
        return Some(match source_alias(provider, function) {
            Some(_) => native(
                ExtenderFamily::PapyrusUtil,
                PRINCIPAL_STORAGE_SERVICE,
                "an exact engine-owned global-value adapter targets byro.storage; ObjKey must be None",
            ),
            None => unsupported(
                ExtenderFamily::PapyrusUtil,
                "this StorageUtil function has no exact engine alias; object-scoped values require extension components, and pluck/file/list/cross-principal semantics remain unavailable",
            ),
        });
    }
    if provider.eq_ignore_ascii_case("JsonUtil") {
        return Some(unsupported(
            ExtenderFamily::PapyrusUtil,
            "arbitrary host-file JSON access is outside the sandbox; use principal storage or packaged content",
        ));
    }
    if matches_ignore_ascii_case(
        provider,
        &["JValue", "JMap", "JArray", "JFormMap", "JIntMap", "JDB"],
    ) {
        return Some(if source_alias(provider, function).is_some() {
            mapped(
                ExtenderFamily::JContainers,
                LEGACY_CONTAINERS_SERVICE,
                "an exact core typed-container alias targets the persisted principal-local object service",
            )
        } else {
            unsupported(
                ExtenderFamily::JContainers,
                "the JContainers provider is recognized, but this function has no exact engine adapter; file JSON, Lua, path solving, form/int-keyed maps, and global cross-mod databases remain unavailable",
            )
        });
    }
    if provider.eq_ignore_ascii_case("ModEvent") {
        return Some(if source_alias(provider, function).is_some() {
            native(
                ExtenderFamily::Skse,
                EVENT_SERVICE,
                "an exact engine-owned ModEvent handle adapter targets the bounded shared compatibility bus",
            )
        } else {
            unsupported(
                ExtenderFamily::Skse,
                "the ModEvent provider is recognized, but this function has no exact engine mapping",
            )
        });
    }
    if provider.eq_ignore_ascii_case("Input") {
        return Some(if source_alias(provider, function).is_some() {
            native(
                ExtenderFamily::Shared,
                INPUT_SERVICE,
                "an exact read-only alias targets the engine's current action-binding snapshot",
            )
        } else if matches_ignore_ascii_case(
            function,
            &[
                "GetMappedKey",
                "GetMappedControl",
                "IsKeyPressed",
                "TapKey",
                "HoldKey",
                "ReleaseKey",
                "GetNumKeysPressed",
                "GetNthKeyPressed",
            ],
        ) {
            unsupported(
                ExtenderFamily::Shared,
                "physical-key polling/injection is not exposed; subscribe to normalized manifest input actions",
            )
        } else {
            unsupported(
                ExtenderFamily::Shared,
                "the Input provider is recognized, but this function has no engine semantic mapping",
            )
        });
    }
    if provider.eq_ignore_ascii_case("UI") {
        return Some(if source_alias(provider, function).is_some() {
            native(
                ExtenderFamily::Shared,
                UI_SERVICE,
                "an exact read-only alias targets the active engine-owned menu snapshot",
            )
        } else {
            unsupported(
                ExtenderFamily::Shared,
                "arbitrary Scaleform object access and menu mutation are not exposed; use the future isolated UI contribution service",
            )
        });
    }
    None
}

/// Classify extender-added instance functions whose declaring type is erased
/// from a `callmethod` instruction. Exact names are used to avoid treating
/// ordinary mod methods as compatibility calls.
pub fn classify_method_call(function: &str) -> Option<CompatibilityMatch> {
    if method_source_alias(function).is_some() {
        return Some(mapped(
            ExtenderFamily::Skse,
            EVENT_SERVICE,
            "an exact source adapter targets the shared engine ModEvent compatibility bus",
        ));
    }
    if matches_ignore_ascii_case(
        function,
        &["RegisterForKey", "UnregisterForKey", "UnregisterForAllKeys"],
    ) {
        return Some(mapped(
            ExtenderFamily::Shared,
            EVENT_SERVICE,
            "replace physical key registration with normalized manifest input-action subscriptions",
        ));
    }
    if matches_ignore_ascii_case(function, &["RegisterForMenu", "UnregisterForMenu"]) {
        return Some(unsupported(
            ExtenderFamily::Shared,
            "menu lifecycle aliases await the isolated UI contribution service",
        ));
    }
    None
}

const fn mapped(
    family: ExtenderFamily,
    service: &'static str,
    guidance: &'static str,
) -> CompatibilityMatch {
    CompatibilityMatch {
        family,
        disposition: CompatibilityDisposition::Mapped,
        service: Some(service),
        guidance,
    }
}

const fn native(
    family: ExtenderFamily,
    service: &'static str,
    guidance: &'static str,
) -> CompatibilityMatch {
    CompatibilityMatch {
        family,
        disposition: CompatibilityDisposition::Native,
        service: Some(service),
        guidance,
    }
}

const fn unsupported(family: ExtenderFamily, guidance: &'static str) -> CompatibilityMatch {
    CompatibilityMatch {
        family,
        disposition: CompatibilityDisposition::Unsupported,
        service: None,
        guidance,
    }
}

fn matches_ignore_ascii_case(value: &str, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| value.eq_ignore_ascii_case(candidate))
}

fn is_extender_version_probe(function: &str) -> bool {
    matches_ignore_ascii_case(
        function,
        &[
            "GetVersion",
            "GetVersionMinor",
            "GetVersionBeta",
            "GetVersionRelease",
        ],
    )
}
