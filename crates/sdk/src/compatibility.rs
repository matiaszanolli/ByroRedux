//! Source-API compatibility catalog for extender-era Papyrus calls.
//!
//! This catalog does not pretend that recognizing a legacy call makes its
//! binary ABI available. It identifies the engine semantic service a source
//! adapter must target, or records why the operation is intentionally absent.

use crate::service::{CONTEXT_SERVICE, EVENT_SERVICE, PRINCIPAL_STORAGE_SERVICE};

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
        return Some(mapped(
            ExtenderFamily::PapyrusUtil,
            PRINCIPAL_STORAGE_SERVICE,
            "migrate global values to principal storage and object-scoped values to extension components",
        ));
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
