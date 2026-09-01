//! Source-API compatibility catalog for extender-era Papyrus calls.
//!
//! This catalog does not pretend that recognizing a legacy call makes its
//! binary ABI available. It identifies the engine semantic service a source
//! adapter must target, or records why the operation is intentionally absent.

use crate::component::ExtensionValue;
use crate::event::{legacy_skse_mod_event_id, LegacySkseModEventPayload, PublishEventCommand};
use crate::identity::FormRef;
use crate::identity::{IdentityError, StorageKey};
use crate::service::{
    CONTENT_CATALOG_SERVICE, CONTEXT_SERVICE, EVENT_SERVICE, LEGACY_CONTAINERS_SERVICE,
    PRINCIPAL_STORAGE_SERVICE,
};
use crate::storage::{PrincipalStorageCommand, PrincipalStorageValue};

/// Extender ecosystem that introduced a recognized Papyrus provider/call.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ExtenderFamily {
    Skse,
    F4se,
    Xnvse,
    Obse,
    PapyrusUtil,
    JContainers,
    Shared,
}

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
        return Some(mapped(
            ExtenderFamily::Shared,
            CONTENT_CATALOG_SERVICE,
            "an exact legacy load-order recipe targets the immutable engine content catalog",
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

/// Current source-compatibility disposition.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CompatibilityDisposition {
    /// A source-level adapter is installed and delegates to the named service.
    Native,
    /// An engine semantic service exists, but the legacy call still needs an
    /// adapter or explicit source migration.
    Mapped,
    /// The operation has no safe/equivalent engine contract.
    Unsupported,
}

/// Classification returned for one recognized extender-era call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompatibilityMatch {
    pub family: ExtenderFamily,
    pub disposition: CompatibilityDisposition,
    pub service: Option<&'static str>,
    pub guidance: &'static str,
}

/// Exact semantic target for a legacy source call that can be ported onto an
/// existing engine service without reproducing extender internals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceAlias {
    pub provider: &'static str,
    pub function: &'static str,
    pub service: &'static str,
    pub operation: &'static str,
    pub value_kind: &'static str,
    pub constraint: &'static str,
}

/// Scalar `StorageUtil` call supported by the engine source adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageUtilScalarCall {
    GetInt { missing: i32 },
    HasInt,
    SetInt { value: i32 },
    UnsetInt,
    GetString { missing: String },
    HasString,
    SetString { value: String },
    UnsetString,
}

/// Papyrus-visible result produced by a scalar `StorageUtil` adapter call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageUtilScalarResult {
    Int(i32),
    Bool(bool),
    String(String),
}

/// Executable result of adapting one global scalar `StorageUtil` call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageUtilAdaptation {
    /// Type-isolated, case-folded key in the authenticated principal namespace.
    pub key: StorageKey,
    /// Value returned synchronously to Papyrus.
    pub result: StorageUtilScalarResult,
    /// Deferred engine mutation, absent for read-only calls.
    pub command: Option<PrincipalStorageCommand>,
}

/// Failure to preserve the supported `StorageUtil` scalar contract.
#[derive(Clone, Debug, Eq, thiserror::Error, PartialEq)]
pub enum StorageUtilAdapterError {
    #[error(
        "StorageUtil key cannot be represented by the portable principal-storage grammar: {0}"
    )]
    InvalidKey(#[from] IdentityError),
    #[error("StorageUtil integer value is outside the Papyrus i32 range")]
    IntegerOutOfRange,
    #[error("StorageUtil adapter found an incompatible value at its type-isolated key")]
    TypeMismatch,
}

/// Failure to adapt SKSE's fixed-arity `SendModEvent` call.
#[derive(Clone, Debug, Eq, thiserror::Error, PartialEq)]
pub enum LegacyModEventAdapterError {
    #[error("legacy mod-event name is empty or exceeds the reversible engine bound")]
    InvalidEventName,
    #[error("legacy mod-event payload exceeds the bounded engine event contract")]
    PayloadTooLarge,
}

/// Adapt `Form`/`Alias`/`ActiveMagicEffect.SendModEvent` to the shared,
/// engine-owned event service.
pub fn adapt_legacy_send_mod_event(
    event_name: &str,
    string_arg: String,
    number_arg: f32,
    sender: Option<FormRef>,
) -> Result<PublishEventCommand, LegacyModEventAdapterError> {
    let event =
        legacy_skse_mod_event_id(event_name).ok_or(LegacyModEventAdapterError::InvalidEventName)?;
    let payload = LegacySkseModEventPayload::new(string_arg, number_arg, sender)
        .encode()
        .ok_or(LegacyModEventAdapterError::PayloadTooLarge)?;
    PublishEventCommand::new(event, payload).ok_or(LegacyModEventAdapterError::PayloadTooLarge)
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

/// Resolve the first engine-backed PapyrusUtil source aliases.
///
/// `StorageUtil` namespaces values by `(object, key, type)`, while
/// `byro.storage` is private to one extension principal and keys each value
/// once. These aliases therefore cover only global (`ObjKey == None`) integer
/// and string values whose names fit the portable storage-key grammar. The
/// executable adapter below case-folds and type-namespaces those keys. Its
/// writes preserve synchronous return and same-callback visibility through the
/// host transaction overlay. Object-scoped values require extension
/// components; floats, Forms, pluck, file, list, and cross-principal sharing
/// remain unsupported until an engine service can honor their full contracts.
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
    if !provider.eq_ignore_ascii_case("StorageUtil") {
        return None;
    }
    let (function, operation, value_kind) = if function.eq_ignore_ascii_case("GetIntValue") {
        ("GetIntValue", "storage.get", "signed")
    } else if function.eq_ignore_ascii_case("HasIntValue") {
        ("HasIntValue", "storage.get", "signed")
    } else if function.eq_ignore_ascii_case("SetIntValue") {
        ("SetIntValue", "storage.queue-set/delete", "signed")
    } else if function.eq_ignore_ascii_case("UnsetIntValue") {
        ("UnsetIntValue", "storage.get+queue-delete", "signed")
    } else if function.eq_ignore_ascii_case("GetStringValue") {
        ("GetStringValue", "storage.get", "text")
    } else if function.eq_ignore_ascii_case("HasStringValue") {
        ("HasStringValue", "storage.get", "text")
    } else if function.eq_ignore_ascii_case("SetStringValue") {
        ("SetStringValue", "storage.queue-set/delete", "text")
    } else if function.eq_ignore_ascii_case("UnsetStringValue") {
        ("UnsetStringValue", "storage.get+queue-delete", "text")
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

fn legacy_container_source_alias(provider: &str, function: &str) -> Option<SourceAlias> {
    let (provider, function, operation, value_kind) = if provider.eq_ignore_ascii_case("JValue") {
        if function.eq_ignore_ascii_case("count") {
            ("JValue", "count", "legacy-containers.count", "signed")
        } else if function.eq_ignore_ascii_case("clear") {
            ("JValue", "clear", "legacy-containers.clear", "none")
        } else if function.eq_ignore_ascii_case("release") {
            ("JValue", "release", "legacy-containers.release", "none")
        } else {
            return None;
        }
    } else if provider.eq_ignore_ascii_case("JArray") {
        if function.eq_ignore_ascii_case("object") {
            (
                "JArray",
                "object",
                "legacy-containers.array-create",
                "handle",
            )
        } else if function.eq_ignore_ascii_case("count") {
            ("JArray", "count", "legacy-containers.count", "signed")
        } else if function.eq_ignore_ascii_case("clear") {
            ("JArray", "clear", "legacy-containers.clear", "none")
        } else if function.eq_ignore_ascii_case("eraseIndex") {
            (
                "JArray",
                "eraseIndex",
                "legacy-containers.array-erase",
                "none",
            )
        } else if function.eq_ignore_ascii_case("addInt") {
            ("JArray", "addInt", "legacy-containers.array-add", "signed")
        } else if function.eq_ignore_ascii_case("addFlt") {
            ("JArray", "addFlt", "legacy-containers.array-add", "float")
        } else if function.eq_ignore_ascii_case("addStr") {
            ("JArray", "addStr", "legacy-containers.array-add", "text")
        } else if function.eq_ignore_ascii_case("addForm") {
            ("JArray", "addForm", "legacy-containers.array-add", "form")
        } else if function.eq_ignore_ascii_case("addObj") {
            ("JArray", "addObj", "legacy-containers.array-add", "handle")
        } else if function.eq_ignore_ascii_case("getInt") {
            ("JArray", "getInt", "legacy-containers.array-get", "signed")
        } else if function.eq_ignore_ascii_case("getFlt") {
            ("JArray", "getFlt", "legacy-containers.array-get", "float")
        } else if function.eq_ignore_ascii_case("getStr") {
            ("JArray", "getStr", "legacy-containers.array-get", "text")
        } else if function.eq_ignore_ascii_case("getForm") {
            ("JArray", "getForm", "legacy-containers.array-get", "form")
        } else if function.eq_ignore_ascii_case("getObj") {
            ("JArray", "getObj", "legacy-containers.array-get", "handle")
        } else if function.eq_ignore_ascii_case("setInt") {
            ("JArray", "setInt", "legacy-containers.array-set", "signed")
        } else if function.eq_ignore_ascii_case("setFlt") {
            ("JArray", "setFlt", "legacy-containers.array-set", "float")
        } else if function.eq_ignore_ascii_case("setStr") {
            ("JArray", "setStr", "legacy-containers.array-set", "text")
        } else if function.eq_ignore_ascii_case("setForm") {
            ("JArray", "setForm", "legacy-containers.array-set", "form")
        } else if function.eq_ignore_ascii_case("setObj") {
            ("JArray", "setObj", "legacy-containers.array-set", "handle")
        } else {
            return None;
        }
    } else if provider.eq_ignore_ascii_case("JMap") {
        if function.eq_ignore_ascii_case("object") {
            ("JMap", "object", "legacy-containers.map-create", "handle")
        } else if function.eq_ignore_ascii_case("count") {
            ("JMap", "count", "legacy-containers.count", "signed")
        } else if function.eq_ignore_ascii_case("clear") {
            ("JMap", "clear", "legacy-containers.clear", "none")
        } else if function.eq_ignore_ascii_case("hasKey") {
            ("JMap", "hasKey", "legacy-containers.map-has-key", "bool")
        } else if function.eq_ignore_ascii_case("removeKey") {
            ("JMap", "removeKey", "legacy-containers.map-remove", "none")
        } else if function.eq_ignore_ascii_case("setInt") {
            ("JMap", "setInt", "legacy-containers.map-set", "signed")
        } else if function.eq_ignore_ascii_case("setFlt") {
            ("JMap", "setFlt", "legacy-containers.map-set", "float")
        } else if function.eq_ignore_ascii_case("setStr") {
            ("JMap", "setStr", "legacy-containers.map-set", "text")
        } else if function.eq_ignore_ascii_case("setForm") {
            ("JMap", "setForm", "legacy-containers.map-set", "form")
        } else if function.eq_ignore_ascii_case("setObj") {
            ("JMap", "setObj", "legacy-containers.map-set", "handle")
        } else if function.eq_ignore_ascii_case("getInt") {
            ("JMap", "getInt", "legacy-containers.map-get", "signed")
        } else if function.eq_ignore_ascii_case("getFlt") {
            ("JMap", "getFlt", "legacy-containers.map-get", "float")
        } else if function.eq_ignore_ascii_case("getStr") {
            ("JMap", "getStr", "legacy-containers.map-get", "text")
        } else if function.eq_ignore_ascii_case("getForm") {
            ("JMap", "getForm", "legacy-containers.map-get", "form")
        } else if function.eq_ignore_ascii_case("getObj") {
            ("JMap", "getObj", "legacy-containers.map-get", "handle")
        } else {
            return None;
        }
    } else {
        return None;
    };
    Some(SourceAlias {
        provider,
        function,
        service: LEGACY_CONTAINERS_SERVICE,
        operation,
        value_kind,
        constraint: "principal-local; <=256 objects; <=4096 aggregate entries; <=4096 UTF-8 bytes per key/string",
    })
}

/// Execute the engine recipe for a supported global scalar `StorageUtil` call.
///
/// The caller supplies the current value from the callback transaction overlay
/// and queues the returned command through `byro.storage`. Integer and string
/// keys are kept separate exactly as in `StorageUtil`, and names are folded to
/// ASCII lowercase because the legacy API treats value names case-insensitively.
pub fn adapt_storage_util_global_scalar(
    key_name: &str,
    call: StorageUtilScalarCall,
    current: Option<&PrincipalStorageValue>,
) -> Result<StorageUtilAdaptation, StorageUtilAdapterError> {
    let integer = matches!(
        &call,
        StorageUtilScalarCall::GetInt { .. }
            | StorageUtilScalarCall::HasInt
            | StorageUtilScalarCall::SetInt { .. }
            | StorageUtilScalarCall::UnsetInt
    );
    let prefix = if integer {
        "storageutil.int:"
    } else {
        "storageutil.string:"
    };
    let key = StorageKey::new(format!("{prefix}{}", key_name.to_ascii_lowercase()))?;

    let (result, command) = match call {
        StorageUtilScalarCall::GetInt { missing } => {
            let value = match current {
                Some(PrincipalStorageValue::I64(value)) => {
                    i32::try_from(*value).map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?
                }
                Some(_) => return Err(StorageUtilAdapterError::TypeMismatch),
                None => missing,
            };
            (StorageUtilScalarResult::Int(value), None)
        }
        StorageUtilScalarCall::HasInt => (
            StorageUtilScalarResult::Bool(checked_int(current)?.is_some()),
            None,
        ),
        StorageUtilScalarCall::SetInt { value } => {
            let command = if value == 0 {
                PrincipalStorageCommand::Delete { key: key.clone() }
            } else {
                PrincipalStorageCommand::Set {
                    key: key.clone(),
                    value: ExtensionValue::I64(i64::from(value)),
                }
            };
            (StorageUtilScalarResult::Int(value), Some(command))
        }
        StorageUtilScalarCall::UnsetInt => (
            StorageUtilScalarResult::Bool(checked_int(current)?.is_some()),
            Some(PrincipalStorageCommand::Delete { key: key.clone() }),
        ),
        StorageUtilScalarCall::GetString { missing } => {
            let value = match current {
                Some(PrincipalStorageValue::String(value)) => value.clone(),
                Some(_) => return Err(StorageUtilAdapterError::TypeMismatch),
                None => missing,
            };
            (StorageUtilScalarResult::String(value), None)
        }
        StorageUtilScalarCall::HasString => (
            StorageUtilScalarResult::Bool(checked_string(current)?.is_some()),
            None,
        ),
        StorageUtilScalarCall::SetString { value } => {
            let command = if value.is_empty() {
                PrincipalStorageCommand::Delete { key: key.clone() }
            } else {
                PrincipalStorageCommand::Set {
                    key: key.clone(),
                    value: ExtensionValue::String(value.clone()),
                }
            };
            (StorageUtilScalarResult::String(value), Some(command))
        }
        StorageUtilScalarCall::UnsetString => (
            StorageUtilScalarResult::Bool(checked_string(current)?.is_some()),
            Some(PrincipalStorageCommand::Delete { key: key.clone() }),
        ),
    };
    Ok(StorageUtilAdaptation {
        key,
        result,
        command,
    })
}

fn checked_int(
    current: Option<&PrincipalStorageValue>,
) -> Result<Option<i32>, StorageUtilAdapterError> {
    match current {
        Some(PrincipalStorageValue::I64(value)) => Ok(Some(
            i32::try_from(*value).map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?,
        )),
        Some(_) => Err(StorageUtilAdapterError::TypeMismatch),
        None => Ok(None),
    }
}

fn checked_string(
    current: Option<&PrincipalStorageValue>,
) -> Result<Option<&str>, StorageUtilAdapterError> {
    match current {
        Some(PrincipalStorageValue::String(value)) => Ok(Some(value)),
        Some(_) => Err(StorageUtilAdapterError::TypeMismatch),
        None => Ok(None),
    }
}

/// Classify a static Papyrus call by provider type and function name.
/// Returns `None` for providers that are not known extender APIs.
pub fn classify_static_call(provider: &str, function: &str) -> Option<CompatibilityMatch> {
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
            Some(_) => mapped(
                ExtenderFamily::PapyrusUtil,
                PRINCIPAL_STORAGE_SERVICE,
                "an exact global-value source alias targets byro.storage; ObjKey must be None",
            ),
            None => unsupported(
                ExtenderFamily::PapyrusUtil,
                "this StorageUtil function has no exact engine alias; object-scoped values require extension components, and adjust/float/Form/file/list/cross-principal semantics remain unavailable",
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
            mapped(
                ExtenderFamily::Skse,
                EVENT_SERVICE,
                "an exact transient ModEvent handle adapter targets the bounded shared engine compatibility bus",
            )
        } else {
            unsupported(
                ExtenderFamily::Skse,
                "the ModEvent provider is recognized, but this function has no exact engine mapping",
            )
        });
    }
    if provider.eq_ignore_ascii_case("Input") {
        return Some(
            if matches_ignore_ascii_case(
                function,
                &[
                    "GetMappedKey",
                    "IsKeyPressed",
                    "TapKey",
                    "HoldKey",
                    "ReleaseKey",
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
            },
        );
    }
    if provider.eq_ignore_ascii_case("UI") {
        return Some(unsupported(
            ExtenderFamily::Shared,
            "arbitrary Scaleform object access is not exposed; use the future isolated UI contribution service",
        ));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_and_events_map_to_existing_semantic_services() {
        let storage = classify_static_call("storageutil", "GetIntValue").unwrap();
        assert_eq!(storage.disposition, CompatibilityDisposition::Mapped);
        assert_eq!(storage.service, Some(PRINCIPAL_STORAGE_SERVICE));
        let event = classify_method_call("RegisterForModEvent").unwrap();
        assert_eq!(event.service, Some(EVENT_SERVICE));
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
        assert!(obscript_source_alias("GetSourceModIndex").is_none());
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
        assert!(source_alias("StorageUtil", "AdjustIntValue").is_none());
        assert!(source_alias("StorageUtil", "GetFloatValue").is_none());
        assert!(source_alias("StorageUtil", "FormListAdd").is_none());
        assert_eq!(
            classify_static_call("StorageUtil", "GetFloatValue")
                .unwrap()
                .disposition,
            CompatibilityDisposition::Unsupported
        );
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
            CompatibilityDisposition::Mapped
        );
        assert_eq!(
            source_alias("ModEvent", "Create").unwrap().operation,
            "events.legacy-builder-create"
        );
        assert_eq!(
            source_alias("ModEvent", "PushForm").unwrap().value_kind,
            "form"
        );
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
