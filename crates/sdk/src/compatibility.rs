//! Source-API compatibility catalog for extender-era Papyrus calls.
//!
//! This catalog does not pretend that recognizing a legacy call makes its
//! binary ABI available. It identifies the engine semantic service a source
//! adapter must target, or records why the operation is intentionally absent.

use crate::component::ExtensionValue;
use crate::identity::{IdentityError, StorageKey};
use crate::service::{CONTEXT_SERVICE, EVENT_SERVICE, PRINCIPAL_STORAGE_SERVICE};
use crate::storage::{PrincipalStorageCommand, PrincipalStorageValue};

/// Extender ecosystem that introduced a recognized Papyrus provider/call.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ExtenderFamily {
    Skse,
    F4se,
    PapyrusUtil,
    JContainers,
    Shared,
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
        return Some(mapped(
            ExtenderFamily::JContainers,
            PRINCIPAL_STORAGE_SERVICE,
            "migrate to bounded principal arrays, maps, sets, or entity-attached extension components",
        ));
    }
    if provider.eq_ignore_ascii_case("ModEvent") {
        return Some(mapped(
            ExtenderFamily::Skse,
            EVENT_SERVICE,
            "declare a principal-owned event channel and use bounded typed payloads",
        ));
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
    if matches_ignore_ascii_case(
        function,
        &[
            "RegisterForModEvent",
            "UnregisterForModEvent",
            "UnregisterForAllModEvents",
            "SendModEvent",
        ],
    ) {
        return Some(mapped(
            ExtenderFamily::Skse,
            EVENT_SERVICE,
            "declare the channel in the extension manifest and publish or subscribe through byro.events",
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
