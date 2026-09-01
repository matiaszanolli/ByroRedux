//! Versioned host service and capability discovery contracts.

use std::collections::BTreeMap;

use semver::Version;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::identity::{CapabilityId, CapabilitySet, ServiceId};
use crate::manifest::{ExtensionManifest, ManifestError};

pub use crate::event::{
    INPUT_ACTION_EVENT, INPUT_ACTION_FILTER_FIELD, SESSION_EVENT, SESSION_PHASE_FILTER_FIELD,
};

/// Capability required to emit an attributed host diagnostic.
pub const LOG_WRITE_CAPABILITY: &str = "byro.log.write";
/// Capability required to enqueue mutations to the caller's own components.
pub const COMPONENTS_WRITE_OWN_CAPABILITY: &str = "byro.components.write-own";
/// Capability required for delivery of declared canonical event subscriptions.
pub const EVENTS_SUBSCRIBE_CAPABILITY: &str = "byro.events.subscribe";
/// Capability required to publish bounded principal-owned custom events.
pub const EVENTS_PUBLISH_CAPABILITY: &str = "byro.events.publish";
/// Additional authority required to observe normalized player input actions.
pub const INPUT_ACTIONS_SUBSCRIBE_CAPABILITY: &str = "byro.input.actions.subscribe";
/// Capability required to read the caller's private persistent storage.
pub const STORAGE_READ_OWN_CAPABILITY: &str = "byro.storage.read-own";
/// Capability required to mutate the caller's private persistent storage.
pub const STORAGE_WRITE_OWN_CAPABILITY: &str = "byro.storage.write-own";
/// Capability required to read bounded facts about known live entities.
pub const WORLD_ENTITY_READ_CAPABILITY: &str = "byro.world.entity.read";
/// Capability required to include world transforms in entity projections.
pub const WORLD_TRANSFORM_READ_CAPABILITY: &str = "byro.world.transform.read";
/// Capability required to read callback-local canonical actor values.
pub const ACTOR_VALUES_READ_CAPABILITY: &str = "byro.actor-values.read";
/// Capability required to queue canonical actor-value mutations.
pub const ACTOR_VALUES_WRITE_CAPABILITY: &str = "byro.actor-values.write";
/// Capability required to inspect callback-local authored animation state.
pub const ANIMATION_READ_CAPABILITY: &str = "byro.animation.read";
/// Capability required to request authored IDLE playback on a visible actor.
pub const ANIMATION_PLAY_CAPABILITY: &str = "byro.animation.play";
/// Capability required to inspect callback-local reputation axes.
pub const REPUTATION_READ_CAPABILITY: &str = "byro.reputation.read";
/// Capability required to queue canonical fame/infamy mutations.
pub const REPUTATION_WRITE_CAPABILITY: &str = "byro.reputation.write";
/// Capability required to read callback-local inventory/equipment summaries.
pub const INVENTORY_READ_CAPABILITY: &str = "byro.inventory.read";
/// Capability required to read callback-local faction memberships.
pub const FACTIONS_READ_CAPABILITY: &str = "byro.factions.read";
/// Capability required to read callback-local ranked actor perks.
pub const PERKS_READ_CAPABILITY: &str = "byro.perks.read";
/// Capability required to inspect live ambient and scene package selections.
pub const PACKAGES_READ_CAPABILITY: &str = "byro.packages.read";
/// Capability required to request deferred package reevaluation for a visible actor.
pub const PACKAGES_EVALUATE_CAPABILITY: &str = "byro.packages.evaluate";
/// Capability required to query bounded live authored-reference positions.
pub const WORLD_SPATIAL_READ_CAPABILITY: &str = "byro.world.spatial.read";
/// Capability required to inspect the active game-content load order.
pub const CONTENT_CATALOG_READ_CAPABILITY: &str = "byro.content.catalog.read";
/// Capability required to publish and execute manifest-declared console commands.
pub const CONSOLE_REGISTER_CAPABILITY: &str = "byro.console.register";
/// Capability required to read the public engine-settings snapshot.
pub const SETTINGS_READ_CAPABILITY: &str = "byro.settings.read";
/// Capability required to register manifest-declared principal settings.
pub const SETTINGS_REGISTER_CAPABILITY: &str = "byro.settings.register";
/// Capability required to queue writes to the caller's declared settings.
pub const SETTINGS_WRITE_OWN_CAPABILITY: &str = "byro.settings.write-own";
/// Service providing principal and host-contract discovery.
pub const CONTEXT_SERVICE: &str = "byro.context";
/// Service providing bounded attributed diagnostics.
pub const LOGGING_SERVICE: &str = "byro.logging";
/// Service accepting deferred mutations to principal-owned component state.
pub const COMPONENT_STATE_SERVICE: &str = "byro.components";
/// Service delivering canonical engine events.
pub const EVENT_SERVICE: &str = "byro.events";
/// Service providing bounded principal-scoped persistent storage.
pub const PRINCIPAL_STORAGE_SERVICE: &str = "byro.storage";
/// Service providing immutable, callback-local entity projections.
pub const WORLD_PROJECTION_SERVICE: &str = "byro.world";
/// Service providing canonical actor-value reads and deferred mutations.
pub const ACTOR_VALUES_SERVICE: &str = "byro.actor-values";
/// Service providing authored animation state and semantic playback requests.
pub const ANIMATION_SERVICE: &str = "byro.animation";
/// Service providing canonical REPU-backed actor reputation state.
pub const REPUTATION_SERVICE: &str = "byro.reputation";
/// Service providing callback-local portable inventory/equipment summaries.
pub const INVENTORY_SERVICE: &str = "byro.inventory";
/// Service providing callback-local portable faction memberships.
pub const FACTIONS_SERVICE: &str = "byro.factions";
/// Service providing callback-local portable actor perk ranks.
pub const PERKS_SERVICE: &str = "byro.perks";
/// Service providing live package selection state and semantic reevaluation.
pub const PACKAGES_SERVICE: &str = "byro.packages";
/// Service providing bounded spatial queries over live authored references.
pub const WORLD_SPATIAL_SERVICE: &str = "byro.world.spatial";
/// Service providing loaded plugin discovery and portable form qualification.
pub const CONTENT_CATALOG_SERVICE: &str = "byro.content.catalog";
/// Service providing bounded, principal-namespaced console callbacks.
pub const CONSOLE_SERVICE: &str = "byro.console";
/// Service providing stable typed read-only engine settings.
pub const SETTINGS_SERVICE: &str = "byro.settings";
/// WIT world implemented by executable extension components.
pub const EXTENSION_WORLD_SERVICE: &str = "byro.mod-host.extension";
/// Canonical activation event identifier.
pub const ACTIVATE_EVENT: &str = "byro.events.activate";
/// Canonical script-bearing entity load event identifier.
pub const CELL_LOAD_EVENT: &str = "byro.events.cell-load";
/// Canonical combat hit event identifier.
pub const HIT_EVENT: &str = "byro.events.hit";
/// Canonical item equip/unequip transition identifier.
pub const EQUIPMENT_EVENT: &str = "byro.events.equipment-change";
/// Canonical engine-owned recurring callback identifier.
pub const UPDATE_EVENT: &str = "byro.events.update";

/// Semantic version of the SDK contract defined by this crate build.
pub fn current_sdk_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION"))
        .expect("Cargo guarantees that CARGO_PKG_VERSION is valid SemVer")
}

/// One capability that the active host can potentially grant.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CapabilityDescriptor {
    /// Stable authority identifier.
    pub id: CapabilityId,
    /// Concise user-facing description suitable for a grant prompt.
    pub description: String,
}

/// One semantic service implemented by the active host.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ServiceDescriptor {
    /// Stable service identifier.
    pub id: ServiceId,
    /// Implemented semantic contract version.
    pub version: Version,
    /// Capability required to invoke mutating or sensitive operations.
    pub required_capability: Option<CapabilityId>,
}

/// Immutable discovery snapshot used before component compilation and at runtime.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ServiceCatalog {
    sdk_version: Version,
    capabilities: BTreeMap<CapabilityId, CapabilityDescriptor>,
    services: BTreeMap<ServiceId, ServiceDescriptor>,
}

/// Catalog construction errors caused by duplicate or inconsistent metadata.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum CatalogError {
    /// A capability or service was registered more than once.
    #[error("duplicate {kind} registration {id}")]
    Duplicate { kind: &'static str, id: String },
    /// A service referenced a capability absent from the same catalog.
    #[error("service {service} requires unregistered capability {capability}")]
    UnknownServiceCapability {
        service: ServiceId,
        capability: CapabilityId,
    },
}

/// Why a manifest or effective grant set cannot activate on a host.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum CompatibilityError {
    /// Host-independent manifest validation failed.
    #[error("invalid extension manifest: {0}")]
    InvalidManifest(#[from] ManifestError),
    /// The active SDK release is outside the manifest's accepted range.
    #[error("extension requires SDK {required}, but host provides {actual}")]
    UnsupportedSdk {
        required: semver::VersionReq,
        actual: Version,
    },
    /// A required requested capability is not implemented by this host.
    #[error("host does not implement required capability {0}")]
    MissingRequiredCapability(CapabilityId),
    /// A component's declared WIT world is absent from the host catalog.
    #[error("host does not implement component world service {0}")]
    MissingService(ServiceId),
    /// The host world/service version does not satisfy the component range.
    #[error("component requires service {service} {required}, but host provides {actual}")]
    UnsupportedServiceVersion {
        service: ServiceId,
        required: semver::VersionReq,
        actual: Version,
    },
    /// Policy attempted to grant authority that the package never requested.
    #[error("effective grant {0} was not requested by the extension")]
    UndeclaredGrant(CapabilityId),
    /// Policy attempted to grant authority that the active host cannot enforce.
    #[error("effective grant {0} is not implemented by the host")]
    UnsupportedGrant(CapabilityId),
    /// Policy denied a capability the manifest marked as required.
    #[error("required capability {0} was not granted by policy")]
    MissingRequiredGrant(CapabilityId),
}

impl ServiceCatalog {
    /// Construct an empty catalog for one SDK semantic version.
    pub fn new(sdk_version: Version) -> Self {
        Self {
            sdk_version,
            capabilities: BTreeMap::new(),
            services: BTreeMap::new(),
        }
    }

    /// Return the SDK contract version implemented by the host.
    pub fn sdk_version(&self) -> &Version {
        &self.sdk_version
    }

    /// Register grantable authority before the catalog becomes active.
    pub fn register_capability(
        &mut self,
        descriptor: CapabilityDescriptor,
    ) -> Result<(), CatalogError> {
        let id = descriptor.id.clone();
        if self.capabilities.contains_key(&id) {
            return Err(CatalogError::Duplicate {
                kind: "capability",
                id: id.to_string(),
            });
        }
        self.capabilities.insert(id, descriptor);
        Ok(())
    }

    /// Register a semantic service and verify its capability metadata.
    pub fn register_service(&mut self, descriptor: ServiceDescriptor) -> Result<(), CatalogError> {
        if let Some(capability) = &descriptor.required_capability {
            if !self.capabilities.contains_key(capability) {
                return Err(CatalogError::UnknownServiceCapability {
                    service: descriptor.id,
                    capability: capability.clone(),
                });
            }
        }
        let id = descriptor.id.clone();
        if self.services.contains_key(&id) {
            return Err(CatalogError::Duplicate {
                kind: "service",
                id: id.to_string(),
            });
        }
        self.services.insert(id, descriptor);
        Ok(())
    }

    /// Return whether the active host can enforce the named capability.
    pub fn supports_capability(&self, id: &str) -> bool {
        self.capabilities.contains_key(id)
    }

    /// Return the active version of a service, when implemented.
    pub fn service_version(&self, id: &str) -> Option<&Version> {
        self.services.get(id).map(|descriptor| &descriptor.version)
    }

    /// Validate host compatibility before any component bytes are compiled.
    pub fn check_manifest(&self, manifest: &ExtensionManifest) -> Result<(), CompatibilityError> {
        manifest.validate()?;
        if !manifest.sdk.matches(&self.sdk_version) {
            return Err(CompatibilityError::UnsupportedSdk {
                required: manifest.sdk.clone(),
                actual: self.sdk_version.clone(),
            });
        }
        for request in &manifest.capabilities {
            if request.required && !self.capabilities.contains_key(&request.id) {
                return Err(CompatibilityError::MissingRequiredCapability(
                    request.id.clone(),
                ));
            }
        }
        for component in &manifest.components {
            let Some(actual) = self.service_version(component.world.as_str()) else {
                return Err(CompatibilityError::MissingService(component.world.clone()));
            };
            if !component.world_version.matches(actual) {
                return Err(CompatibilityError::UnsupportedServiceVersion {
                    service: component.world.clone(),
                    required: component.world_version.clone(),
                    actual: actual.clone(),
                });
            }
        }
        Ok(())
    }

    /// Validate the policy-selected effective grants for one compatible package.
    pub fn check_grants(
        &self,
        manifest: &ExtensionManifest,
        grants: &CapabilitySet,
    ) -> Result<(), CompatibilityError> {
        self.check_manifest(manifest)?;
        for grant in grants.iter() {
            if manifest.capability(grant.as_str()).is_none() {
                return Err(CompatibilityError::UndeclaredGrant(grant.clone()));
            }
            if !self.capabilities.contains_key(grant) {
                return Err(CompatibilityError::UnsupportedGrant(grant.clone()));
            }
        }
        for request in &manifest.capabilities {
            if request.required && !grants.contains(request.id.as_str()) {
                return Err(CompatibilityError::MissingRequiredGrant(request.id.clone()));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use semver::VersionReq;

    use super::*;
    use crate::identity::{ComponentId, ExtensionId};
    use crate::manifest::{
        CapabilityRequest, ExecutableComponent, ExtensionManifest, EXTENSION_MANIFEST_VERSION,
    };

    fn manifest(sdk: &str, required: bool) -> ExtensionManifest {
        ExtensionManifest {
            manifest_version: EXTENSION_MANIFEST_VERSION,
            id: ExtensionId::new("org.example.catalog-test").unwrap(),
            name: "Catalog test".to_owned(),
            version: Version::new(1, 0, 0),
            sdk: VersionReq::parse(sdk).unwrap(),
            dependencies: Vec::new(),
            components: vec![ExecutableComponent {
                id: ComponentId::new("runtime").unwrap(),
                path: "runtime.wasm".to_owned(),
                world: ServiceId::new("byro.mod-host.extension").unwrap(),
                world_version: VersionReq::parse("^0.1").unwrap(),
            }],
            capabilities: vec![CapabilityRequest {
                id: CapabilityId::new(LOG_WRITE_CAPABILITY).unwrap(),
                required,
            }],
            subscriptions: Vec::new(),
            component_schemas: Vec::new(),
            console_commands: Vec::new(),
            settings: Vec::new(),
            principal_storage_schema: None,
        }
    }

    fn catalog() -> ServiceCatalog {
        let mut catalog = ServiceCatalog::new(Version::new(0, 1, 0));
        catalog
            .register_capability(CapabilityDescriptor {
                id: CapabilityId::new(LOG_WRITE_CAPABILITY).unwrap(),
                description: "Emit bounded attributed diagnostics".to_owned(),
            })
            .unwrap();
        catalog
            .register_service(ServiceDescriptor {
                id: ServiceId::new(EXTENSION_WORLD_SERVICE).unwrap(),
                version: Version::new(0, 1, 0),
                required_capability: None,
            })
            .unwrap();
        catalog
    }

    #[test]
    fn sdk_ranges_and_required_capabilities_are_checked_before_activation() {
        let catalog = catalog();
        catalog.check_manifest(&manifest("^0.1", true)).unwrap();
        assert!(matches!(
            catalog.check_manifest(&manifest(">=1.0", true)),
            Err(CompatibilityError::UnsupportedSdk { .. })
        ));
    }

    #[test]
    fn component_world_versions_are_resolved_before_compilation() {
        let catalog = catalog();
        let mut incompatible = manifest("^0.1", false);
        incompatible.components[0].world_version = VersionReq::parse(">=1.0").unwrap();
        assert!(matches!(
            catalog.check_manifest(&incompatible),
            Err(CompatibilityError::UnsupportedServiceVersion { .. })
        ));
    }

    #[test]
    fn grants_must_be_supported_requested_and_satisfy_required_requests() {
        let catalog = catalog();
        let manifest = manifest("^0.1", true);
        assert!(matches!(
            catalog.check_grants(&manifest, &CapabilitySet::new()),
            Err(CompatibilityError::MissingRequiredGrant(_))
        ));

        let mut grants = CapabilitySet::new();
        grants.grant(LOG_WRITE_CAPABILITY).unwrap();
        catalog.check_grants(&manifest, &grants).unwrap();

        grants.grant("byro.world.raw-memory").unwrap();
        assert!(matches!(
            catalog.check_grants(&manifest, &grants),
            Err(CompatibilityError::UndeclaredGrant(_))
        ));
    }
}
