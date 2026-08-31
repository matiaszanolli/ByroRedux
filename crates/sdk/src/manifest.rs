//! Pure extension-package declarations shared by loaders, hosts, and tools.

use std::collections::BTreeSet;

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::component::ComponentFieldDeclaration;
use crate::identity::{
    CapabilityId, ComponentId, ComponentSchemaId, EventId, ExtensionId, ServiceId,
};

/// The only manifest schema understood by this SDK release.
pub const EXTENSION_MANIFEST_VERSION: u32 = 1;

const MAX_DISPLAY_NAME_BYTES: usize = 256;
const MAX_COMPONENTS: usize = 32;
const MAX_DEPENDENCIES: usize = 256;
const MAX_CAPABILITIES: usize = 256;
const MAX_SUBSCRIPTIONS: usize = 256;
const MAX_SCHEMAS: usize = 256;
const MAX_SCHEMA_FIELDS: usize = 128;
const MAX_PACKAGE_PATH_BYTES: usize = 512;
const MAX_FILTER_VALUE_BYTES: usize = 512;
/// Smallest supported recurring callback cadence.
pub const MIN_RECURRING_UPDATE_INTERVAL_MS: u32 = 16;
/// Largest supported recurring callback cadence.
pub const MAX_RECURRING_UPDATE_INTERVAL_MS: u32 = 3_600_000;
const UPDATE_EVENT_ID: &str = "byro.events.update";

/// Versioned package contract for sandboxed executable extensions.
///
/// Parsing this value does not grant authority. The loader must call
/// [`Self::validate`], resolve dependencies, and apply host/user policy before
/// compiling or instantiating any component.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionManifest {
    /// Manifest schema version, independent of the extension and SDK versions.
    pub manifest_version: u32,
    /// Stable package identity, independent of path or load order.
    pub id: ExtensionId,
    /// Human-facing package name.
    pub name: String,
    /// Package semantic version.
    pub version: Version,
    /// SDK releases with which this package contract is compatible.
    pub sdk: VersionReq,
    /// Required and optional package dependencies.
    #[serde(default)]
    pub dependencies: Vec<ExtensionDependency>,
    /// WebAssembly components owned by this package principal.
    pub components: Vec<ExecutableComponent>,
    /// Host capabilities requested from policy. Requests are never grants.
    #[serde(default)]
    pub capabilities: Vec<CapabilityRequest>,
    /// Canonical events requested before activation.
    #[serde(default)]
    pub subscriptions: Vec<EventSubscription>,
    /// Engine-owned dynamic component schemas declared by this principal.
    #[serde(default)]
    pub component_schemas: Vec<ComponentSchemaDeclaration>,
    /// Version of principal-scoped persistent storage, when used.
    #[serde(default)]
    pub principal_storage_schema: Option<u32>,
}

/// A package dependency and compatible version range.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionDependency {
    /// Stable identity of the dependency.
    pub id: ExtensionId,
    /// Acceptable installed versions.
    pub version: VersionReq,
    /// Whether activation may continue when no compatible provider exists.
    #[serde(default)]
    pub optional: bool,
}

/// One executable artifact owned by the package principal.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutableComponent {
    /// Manifest-local component identity.
    pub id: ComponentId,
    /// Package-relative component path.
    pub path: String,
    /// WIT world/service contract implemented by the component.
    pub world: ServiceId,
    /// Compatible world contract versions.
    pub world_version: VersionReq,
}

/// A capability requested by a package.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityRequest {
    /// Namespaced authority requested from the host policy.
    pub id: CapabilityId,
    /// Whether absence prevents activation.
    #[serde(default = "default_true")]
    pub required: bool,
}

const fn default_true() -> bool {
    true
}

/// A declarative subscription to one canonical event.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventSubscription {
    /// Canonical event identifier.
    pub event: EventId,
    /// Bounded equality filters interpreted by the event service.
    #[serde(default)]
    pub filters: Vec<EventFilter>,
    /// Recurring cadence for `byro.events.update`; absent for immediate
    /// engine events.
    #[serde(default)]
    pub interval_millis: Option<u32>,
}

/// One bounded equality predicate attached to an event subscription.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventFilter {
    /// Namespaced payload field selected by the event contract.
    pub field: ServiceId,
    /// Manifest-authored comparison value, interpreted by that field contract.
    pub equals: String,
}

/// Minimal registration record for a dynamic extension-component schema.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentSchemaDeclaration {
    /// Principal-owned schema identity.
    pub id: ComponentSchemaId,
    /// Positive schema version persisted with rows and save data.
    pub version: u32,
    /// Ordered field table used by compact sandbox indices.
    pub fields: Vec<ComponentFieldDeclaration>,
}

/// Structural problems rejected before dependency resolution or compilation.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ManifestError {
    /// The manifest schema is not implemented by this SDK.
    #[error("unsupported extension manifest version {actual}; expected {expected}")]
    UnsupportedManifestVersion { actual: u32, expected: u32 },
    /// The display name was empty or only whitespace.
    #[error("extension display name must not be empty")]
    EmptyName,
    /// Control characters could spoof prompts, logs, or terminal output.
    #[error("extension display name must not contain control characters")]
    UnsafeName,
    /// An executable extension package declared no component artifacts.
    #[error("extension manifest must declare at least one executable component")]
    MissingComponent,
    /// A bounded manifest collection or string exceeded its limit.
    #[error("{field} exceeds its limit of {maximum}")]
    LimitExceeded { field: &'static str, maximum: usize },
    /// A package attempted to depend on itself.
    #[error("extension {0} must not depend on itself")]
    SelfDependency(ExtensionId),
    /// A stable identifier was repeated where the contract requires uniqueness.
    #[error("duplicate {kind} identifier {id}")]
    DuplicateId { kind: &'static str, id: String },
    /// A package-relative component path was unsafe or non-portable.
    #[error("invalid component path {path:?}: {reason}")]
    InvalidComponentPath { path: String, reason: &'static str },
    /// Schema versions are one-based; zero is reserved for absence.
    #[error("{kind} schema version must be positive")]
    ZeroSchemaVersion { kind: &'static str },
    /// Dynamic schemas must declare at least one typed field.
    #[error("component schema {0} must declare at least one field")]
    EmptyComponentSchema(ComponentSchemaId),
    /// Recurring updates require an explicit bounded cadence.
    #[error("event {0} requires interval_millis")]
    MissingRecurringInterval(EventId),
    /// Immediate events cannot carry a recurring cadence.
    #[error("event {0} does not accept interval_millis")]
    UnexpectedRecurringInterval(EventId),
    /// A recurring cadence was outside the engine's supported range.
    #[error("event {event} interval {actual}ms is outside {minimum}..={maximum}ms")]
    InvalidRecurringInterval {
        event: EventId,
        actual: u32,
        minimum: u32,
        maximum: u32,
    },
}

impl ExtensionManifest {
    /// Validate every host-independent manifest invariant.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.manifest_version != EXTENSION_MANIFEST_VERSION {
            return Err(ManifestError::UnsupportedManifestVersion {
                actual: self.manifest_version,
                expected: EXTENSION_MANIFEST_VERSION,
            });
        }
        if self.name.trim().is_empty() {
            return Err(ManifestError::EmptyName);
        }
        if self.name.chars().any(char::is_control) {
            return Err(ManifestError::UnsafeName);
        }
        check_len("display name", self.name.len(), MAX_DISPLAY_NAME_BYTES)?;
        check_len("components", self.components.len(), MAX_COMPONENTS)?;
        check_len("dependencies", self.dependencies.len(), MAX_DEPENDENCIES)?;
        check_len("capabilities", self.capabilities.len(), MAX_CAPABILITIES)?;
        check_len("subscriptions", self.subscriptions.len(), MAX_SUBSCRIPTIONS)?;
        check_len(
            "component schemas",
            self.component_schemas.len(),
            MAX_SCHEMAS,
        )?;
        if self.components.is_empty() {
            return Err(ManifestError::MissingComponent);
        }

        let mut dependencies = BTreeSet::new();
        for dependency in &self.dependencies {
            if dependency.id == self.id {
                return Err(ManifestError::SelfDependency(self.id.clone()));
            }
            insert_unique(&mut dependencies, "dependency", &dependency.id)?;
        }

        let mut components = BTreeSet::new();
        for component in &self.components {
            insert_unique(&mut components, "component", &component.id)?;
            validate_package_path(&component.path)?;
        }

        let mut capabilities = BTreeSet::new();
        for request in &self.capabilities {
            insert_unique(&mut capabilities, "capability", &request.id)?;
        }

        let mut subscriptions = BTreeSet::new();
        for subscription in &self.subscriptions {
            insert_unique(
                &mut subscriptions,
                "event subscription",
                &subscription.event,
            )?;
            for filter in &subscription.filters {
                check_len(
                    "event filter value",
                    filter.equals.len(),
                    MAX_FILTER_VALUE_BYTES,
                )?;
            }
            match (
                subscription.event.as_str() == UPDATE_EVENT_ID,
                subscription.interval_millis,
            ) {
                (true, None) => {
                    return Err(ManifestError::MissingRecurringInterval(
                        subscription.event.clone(),
                    ));
                }
                (false, Some(_)) => {
                    return Err(ManifestError::UnexpectedRecurringInterval(
                        subscription.event.clone(),
                    ));
                }
                (true, Some(actual))
                    if !(MIN_RECURRING_UPDATE_INTERVAL_MS..=MAX_RECURRING_UPDATE_INTERVAL_MS)
                        .contains(&actual) =>
                {
                    return Err(ManifestError::InvalidRecurringInterval {
                        event: subscription.event.clone(),
                        actual,
                        minimum: MIN_RECURRING_UPDATE_INTERVAL_MS,
                        maximum: MAX_RECURRING_UPDATE_INTERVAL_MS,
                    });
                }
                _ => {}
            }
        }

        let mut schemas = BTreeSet::new();
        for schema in &self.component_schemas {
            insert_unique(&mut schemas, "component schema", &schema.id)?;
            if schema.version == 0 {
                return Err(ManifestError::ZeroSchemaVersion { kind: "component" });
            }
            if schema.fields.is_empty() {
                return Err(ManifestError::EmptyComponentSchema(schema.id.clone()));
            }
            check_len(
                "component schema fields",
                schema.fields.len(),
                MAX_SCHEMA_FIELDS,
            )?;
            let mut fields = BTreeSet::new();
            for field in &schema.fields {
                insert_unique(&mut fields, "component field", &field.id)?;
            }
        }
        if self.principal_storage_schema == Some(0) {
            return Err(ManifestError::ZeroSchemaVersion {
                kind: "principal storage",
            });
        }
        Ok(())
    }

    /// Return a declared capability request by stable identifier.
    pub fn capability(&self, id: &str) -> Option<&CapabilityRequest> {
        self.capabilities
            .iter()
            .find(|request| request.id.as_str() == id)
    }
}

fn check_len(field: &'static str, actual: usize, maximum: usize) -> Result<(), ManifestError> {
    if actual > maximum {
        Err(ManifestError::LimitExceeded { field, maximum })
    } else {
        Ok(())
    }
}

fn insert_unique<T>(ids: &mut BTreeSet<T>, kind: &'static str, id: &T) -> Result<(), ManifestError>
where
    T: Clone + Ord + ToString,
{
    if ids.insert(id.clone()) {
        Ok(())
    } else {
        Err(ManifestError::DuplicateId {
            kind,
            id: id.to_string(),
        })
    }
}

fn validate_package_path(path: &str) -> Result<(), ManifestError> {
    let invalid = |reason| ManifestError::InvalidComponentPath {
        path: path.to_owned(),
        reason,
    };
    if path.is_empty() {
        return Err(invalid("path must not be empty"));
    }
    if path.len() > MAX_PACKAGE_PATH_BYTES {
        return Err(invalid("path exceeds 512 bytes"));
    }
    if path.starts_with('/') || path.contains('\\') {
        return Err(invalid("path must be relative and use forward slashes"));
    }
    if path.contains(':') || path.bytes().any(|byte| byte.is_ascii_control()) {
        return Err(invalid("path contains a drive prefix or control character"));
    }
    if path
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(invalid("path contains an empty, dot, or parent segment"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> ExtensionManifest {
        ExtensionManifest {
            manifest_version: EXTENSION_MANIFEST_VERSION,
            id: ExtensionId::new("org.example.weather").unwrap(),
            name: "Weather overhaul".to_owned(),
            version: Version::new(1, 2, 0),
            sdk: VersionReq::parse("^0.1").unwrap(),
            dependencies: Vec::new(),
            components: vec![ExecutableComponent {
                id: ComponentId::new("runtime").unwrap(),
                path: "code/weather.wasm".to_owned(),
                world: ServiceId::new("byro.mod-host.extension").unwrap(),
                world_version: VersionReq::parse("^0.1").unwrap(),
            }],
            capabilities: vec![CapabilityRequest {
                id: CapabilityId::new("byro.log.write").unwrap(),
                required: true,
            }],
            subscriptions: Vec::new(),
            component_schemas: Vec::new(),
            principal_storage_schema: Some(1),
        }
    }

    #[test]
    fn valid_manifest_is_accepted() {
        manifest().validate().unwrap();
    }

    #[test]
    fn duplicate_capabilities_and_traversal_paths_are_rejected() {
        let mut duplicate = manifest();
        duplicate
            .capabilities
            .push(duplicate.capabilities[0].clone());
        assert!(matches!(
            duplicate.validate(),
            Err(ManifestError::DuplicateId {
                kind: "capability",
                ..
            })
        ));

        let mut traversal = manifest();
        traversal.components[0].path = "../escape.wasm".to_owned();
        assert!(matches!(
            traversal.validate(),
            Err(ManifestError::InvalidComponentPath { .. })
        ));

        traversal.components[0].path = "C:/outside.wasm".to_owned();
        assert!(matches!(
            traversal.validate(),
            Err(ManifestError::InvalidComponentPath { .. })
        ));

        let mut spoofed = manifest();
        spoofed.name = "Trusted engine\u{1b}[31m".to_owned();
        assert_eq!(spoofed.validate(), Err(ManifestError::UnsafeName));
    }

    #[test]
    fn recurring_update_subscriptions_require_a_bounded_interval() {
        let update = EventId::new(UPDATE_EVENT_ID).unwrap();
        let activate = EventId::new("byro.events.activate").unwrap();

        let mut valid = manifest();
        valid.subscriptions.push(EventSubscription {
            event: update.clone(),
            filters: Vec::new(),
            interval_millis: Some(100),
        });
        valid.validate().unwrap();

        let mut missing = manifest();
        missing.subscriptions.push(EventSubscription {
            event: update.clone(),
            filters: Vec::new(),
            interval_millis: None,
        });
        assert_eq!(
            missing.validate(),
            Err(ManifestError::MissingRecurringInterval(update.clone()))
        );

        let mut too_fast = manifest();
        too_fast.subscriptions.push(EventSubscription {
            event: update.clone(),
            filters: Vec::new(),
            interval_millis: Some(MIN_RECURRING_UPDATE_INTERVAL_MS - 1),
        });
        assert!(matches!(
            too_fast.validate(),
            Err(ManifestError::InvalidRecurringInterval { actual: 15, .. })
        ));

        let mut too_slow = manifest();
        too_slow.subscriptions.push(EventSubscription {
            event: update,
            filters: Vec::new(),
            interval_millis: Some(MAX_RECURRING_UPDATE_INTERVAL_MS + 1),
        });
        assert!(matches!(
            too_slow.validate(),
            Err(ManifestError::InvalidRecurringInterval {
                actual: 3_600_001,
                ..
            })
        ));

        let mut misplaced = manifest();
        misplaced.subscriptions.push(EventSubscription {
            event: activate.clone(),
            filters: Vec::new(),
            interval_millis: Some(100),
        });
        assert_eq!(
            misplaced.validate(),
            Err(ManifestError::UnexpectedRecurringInterval(activate))
        );
    }
}
