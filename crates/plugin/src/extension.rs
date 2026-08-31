//! Executable-extension manifest parsing and dependency-set resolution.
//!
//! This deliberately remains separate from record-oriented [`PluginManifest`]
//! while sharing the same dependency graph primitive used for record conflict
//! ancestry.

use std::collections::{BTreeMap, BTreeSet};

use byroredux_sdk::identity::ExtensionId;
use byroredux_sdk::manifest::{ExtensionManifest, ManifestError};
use byroredux_sdk::service::{CompatibilityError, ServiceCatalog};
use thiserror::Error;

use crate::resolver::DependencyGraph;

/// Why an executable extension set could not be activated.
#[derive(Debug, Error)]
pub enum ExtensionResolutionError {
    /// TOML could not be decoded into the SDK manifest contract.
    #[error("failed to parse extension manifest: {0}")]
    Parse(#[from] toml::de::Error),
    /// Host-independent manifest validation failed.
    #[error("extension manifest is invalid: {0}")]
    InvalidManifest(#[from] ManifestError),
    /// Two input packages claimed the same stable identity.
    #[error("duplicate extension identity {0}")]
    DuplicateIdentity(ExtensionId),
    /// A required dependency was not present.
    #[error("extension {extension} requires missing dependency {dependency}")]
    MissingDependency {
        extension: ExtensionId,
        dependency: ExtensionId,
    },
    /// An installed dependency was outside the declared compatible range.
    #[error(
        "extension {extension} requires {dependency} {required}, but installed version is {actual}"
    )]
    IncompatibleDependency {
        extension: ExtensionId,
        dependency: ExtensionId,
        required: semver::VersionReq,
        actual: semver::Version,
    },
    /// The dependency graph contained a construction-order cycle.
    #[error("extension dependency cycle: {}", display_cycle(.path))]
    DependencyCycle { path: Vec<ExtensionId> },
    /// The active host cannot satisfy the package contract.
    #[error("extension {extension} is incompatible with this host: {source}")]
    HostIncompatible {
        extension: ExtensionId,
        source: CompatibilityError,
    },
}

fn display_cycle(cycle: &[ExtensionId]) -> String {
    cycle
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" -> ")
}

/// Validated extension manifests in deterministic dependency-first order.
#[derive(Clone, Debug)]
pub struct ResolvedExtensionSet {
    ordered: Vec<ExtensionManifest>,
}

impl ResolvedExtensionSet {
    /// Parse one pure manifest document. No filesystem access occurs here.
    pub fn parse_manifest(source: &str) -> Result<ExtensionManifest, ExtensionResolutionError> {
        let manifest: ExtensionManifest = toml::from_str(source)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validate, version-check, and dependency-order an immutable package set.
    pub fn resolve(
        manifests: impl IntoIterator<Item = ExtensionManifest>,
        catalog: &ServiceCatalog,
    ) -> Result<Self, ExtensionResolutionError> {
        let mut by_id = BTreeMap::new();
        for manifest in manifests {
            manifest.validate()?;
            catalog.check_manifest(&manifest).map_err(|source| {
                ExtensionResolutionError::HostIncompatible {
                    extension: manifest.id.clone(),
                    source,
                }
            })?;
            let id = manifest.id.clone();
            if by_id.insert(id.clone(), manifest).is_some() {
                return Err(ExtensionResolutionError::DuplicateIdentity(id));
            }
        }

        let mut edges = Vec::with_capacity(by_id.len());
        for manifest in by_id.values() {
            let mut dependencies = BTreeSet::new();
            for dependency in &manifest.dependencies {
                let Some(installed) = by_id.get(&dependency.id) else {
                    if dependency.optional {
                        continue;
                    }
                    return Err(ExtensionResolutionError::MissingDependency {
                        extension: manifest.id.clone(),
                        dependency: dependency.id.clone(),
                    });
                };
                if !dependency.version.matches(&installed.version) {
                    if dependency.optional {
                        continue;
                    }
                    return Err(ExtensionResolutionError::IncompatibleDependency {
                        extension: manifest.id.clone(),
                        dependency: dependency.id.clone(),
                        required: dependency.version.clone(),
                        actual: installed.version.clone(),
                    });
                }
                dependencies.insert(dependency.id.clone());
            }
            edges.push((manifest.id.clone(), dependencies.into_iter().collect()));
        }

        let order = DependencyGraph::new(edges)
            .dependency_order()
            .map_err(|path| ExtensionResolutionError::DependencyCycle { path })?;
        let ordered = order
            .into_iter()
            .map(|id| {
                by_id
                    .remove(&id)
                    .expect("dependency order only contains input IDs")
            })
            .collect();
        Ok(Self { ordered })
    }

    /// Iterate manifests in deterministic dependency-first activation order.
    pub fn manifests(&self) -> &[ExtensionManifest] {
        &self.ordered
    }
}

#[cfg(test)]
mod tests {
    use semver::{Version, VersionReq};

    use super::*;
    use byroredux_sdk::identity::{CapabilityId, ComponentId, ServiceId};
    use byroredux_sdk::manifest::{
        CapabilityRequest, ExecutableComponent, ExtensionDependency, EXTENSION_MANIFEST_VERSION,
    };
    use byroredux_sdk::service::{
        CapabilityDescriptor, ServiceDescriptor, EXTENSION_WORLD_SERVICE, LOG_WRITE_CAPABILITY,
    };

    fn catalog() -> ServiceCatalog {
        let mut catalog = ServiceCatalog::new(Version::new(0, 1, 0));
        catalog
            .register_capability(CapabilityDescriptor {
                id: CapabilityId::new(LOG_WRITE_CAPABILITY).unwrap(),
                description: "test logging".to_owned(),
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

    fn manifest(id: &str, dependencies: &[(&str, &str, bool)]) -> ExtensionManifest {
        ExtensionManifest {
            manifest_version: EXTENSION_MANIFEST_VERSION,
            id: ExtensionId::new(id).unwrap(),
            name: id.to_owned(),
            version: Version::new(1, 0, 0),
            sdk: VersionReq::parse("^0.1").unwrap(),
            dependencies: dependencies
                .iter()
                .map(|(id, version, optional)| ExtensionDependency {
                    id: ExtensionId::new(*id).unwrap(),
                    version: VersionReq::parse(version).unwrap(),
                    optional: *optional,
                })
                .collect(),
            components: vec![ExecutableComponent {
                id: ComponentId::new("runtime").unwrap(),
                path: "runtime.wasm".to_owned(),
                world: ServiceId::new("byro.mod-host.extension").unwrap(),
                world_version: VersionReq::parse("^0.1").unwrap(),
            }],
            capabilities: vec![CapabilityRequest {
                id: CapabilityId::new(LOG_WRITE_CAPABILITY).unwrap(),
                required: false,
            }],
            subscriptions: Vec::new(),
            component_schemas: Vec::new(),
            principal_storage_schema: None,
        }
    }

    #[test]
    fn dependencies_are_ordered_by_the_shared_graph() {
        let resolved = ResolvedExtensionSet::resolve(
            [
                manifest("org.example.c", &[("org.example.b", "^1", false)]),
                manifest("org.example.a", &[]),
                manifest("org.example.b", &[("org.example.a", "^1", false)]),
            ],
            &catalog(),
        )
        .unwrap();
        let ids: Vec<_> = resolved
            .manifests()
            .iter()
            .map(|manifest| manifest.id.as_str())
            .collect();
        assert_eq!(ids, ["org.example.a", "org.example.b", "org.example.c"]);
    }

    #[test]
    fn pure_toml_manifest_parses_through_the_sdk_contract() {
        let source = r#"
manifest_version = 1
id = "org.example.weather"
name = "Weather overhaul"
version = "1.2.0"
sdk = "^0.1"
principal_storage_schema = 1

[[components]]
id = "runtime"
path = "code/weather.wasm"
world = "byro.mod-host.extension"
world_version = "^0.1"

[[capabilities]]
id = "byro.log.write"
required = false
"#;
        let manifest = ResolvedExtensionSet::parse_manifest(source).unwrap();
        assert_eq!(manifest.id.as_str(), "org.example.weather");
        assert_eq!(manifest.components[0].path, "code/weather.wasm");
    }

    #[test]
    fn cycles_missing_dependencies_and_bad_versions_fail_before_activation() {
        let catalog = catalog();
        let cycle = ResolvedExtensionSet::resolve(
            [
                manifest("org.example.a", &[("org.example.b", "^1", false)]),
                manifest("org.example.b", &[("org.example.a", "^1", false)]),
            ],
            &catalog,
        );
        assert!(matches!(
            cycle,
            Err(ExtensionResolutionError::DependencyCycle { .. })
        ));

        let missing = ResolvedExtensionSet::resolve(
            [manifest(
                "org.example.a",
                &[("org.example.missing", "^1", false)],
            )],
            &catalog,
        );
        assert!(matches!(
            missing,
            Err(ExtensionResolutionError::MissingDependency { .. })
        ));

        let mut dependency = manifest("org.example.base", &[]);
        dependency.version = Version::new(2, 0, 0);
        let incompatible = ResolvedExtensionSet::resolve(
            [
                dependency,
                manifest("org.example.addon", &[("org.example.base", "^1", false)]),
            ],
            &catalog,
        );
        assert!(matches!(
            incompatible,
            Err(ExtensionResolutionError::IncompatibleDependency { .. })
        ));
    }
}
