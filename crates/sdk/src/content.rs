//! Bounded discovery of loaded content sources and portable authored forms.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::identity::FormRef;

/// Maximum legal Bethesda-style load order: 254 regular plus 4096 light slots.
pub const MAX_LOADED_PLUGINS: usize = 4_350;
/// Portable upper bound for one plugin basename.
pub const MAX_PLUGIN_NAME_BYTES: usize = 260;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginKind {
    Regular,
    Light,
}

/// One loaded content source in deterministic load-order position.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginInfo {
    name: String,
    source: [u8; 16],
    kind: PluginKind,
    #[serde(default)]
    dependencies: Vec<u32>,
}

impl PluginInfo {
    pub fn new(
        name: impl Into<String>,
        source: [u8; 16],
        kind: PluginKind,
    ) -> Result<Self, ContentCatalogError> {
        let name = name.into();
        if name.is_empty()
            || name.len() > MAX_PLUGIN_NAME_BYTES
            || name.chars().any(char::is_control)
            || name.contains(['/', '\\'])
        {
            return Err(ContentCatalogError::InvalidPluginName(name));
        }
        Ok(Self {
            name,
            source,
            kind,
            dependencies: Vec::new(),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn source(&self) -> [u8; 16] {
        self.source
    }

    pub const fn kind(&self) -> PluginKind {
        self.kind
    }

    /// Catalog ordinals of this plugin's declared masters, in TES4 order.
    pub fn dependencies(&self) -> &[u32] {
        &self.dependencies
    }
}

/// Immutable callback snapshot of the active game-content load order.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentCatalog(Vec<PluginInfo>);

impl ContentCatalog {
    pub fn new(plugins: Vec<PluginInfo>) -> Result<Self, ContentCatalogError> {
        let dependencies = vec![Vec::new(); plugins.len()];
        Self::new_with_dependencies(plugins, dependencies)
    }

    pub fn new_with_dependencies(
        mut plugins: Vec<PluginInfo>,
        dependencies: Vec<Vec<u32>>,
    ) -> Result<Self, ContentCatalogError> {
        if plugins.len() > MAX_LOADED_PLUGINS {
            return Err(ContentCatalogError::PluginBudgetExceeded {
                maximum: MAX_LOADED_PLUGINS,
            });
        }
        if dependencies.len() != plugins.len() {
            return Err(ContentCatalogError::DependencyShapeMismatch);
        }
        let mut names = BTreeSet::new();
        let mut sources = BTreeSet::new();
        for (plugin_index, (plugin, plugin_dependencies)) in
            plugins.iter_mut().zip(dependencies).enumerate()
        {
            let folded = plugin.name.to_ascii_lowercase();
            if !names.insert(folded) {
                return Err(ContentCatalogError::DuplicatePluginName(
                    plugin.name.clone(),
                ));
            }
            if !sources.insert(plugin.source) {
                return Err(ContentCatalogError::DuplicateSource(plugin.source));
            }
            let mut unique_dependencies = BTreeSet::new();
            for dependency in &plugin_dependencies {
                if *dependency as usize >= plugin_index {
                    return Err(ContentCatalogError::InvalidDependency {
                        plugin: u32::try_from(plugin_index)
                            .expect("content catalog is bounded below u32::MAX"),
                        dependency: *dependency,
                    });
                }
                if !unique_dependencies.insert(*dependency) {
                    return Err(ContentCatalogError::DuplicateDependency {
                        plugin: u32::try_from(plugin_index)
                            .expect("content catalog is bounded below u32::MAX"),
                        dependency: *dependency,
                    });
                }
            }
            plugin.dependencies = plugin_dependencies;
        }
        Ok(Self(plugins))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn plugin(&self, index: u32) -> Option<&PluginInfo> {
        self.0.get(index as usize)
    }

    pub fn dependency(&self, plugin: u32, index: u32) -> Option<u32> {
        self.plugin(plugin)?
            .dependencies()
            .get(index as usize)
            .copied()
    }

    pub fn find(&self, name: &str) -> Option<(u32, &PluginInfo)> {
        self.0
            .iter()
            .enumerate()
            .find(|(_, plugin)| plugin.name.eq_ignore_ascii_case(name))
            .map(|(index, plugin)| {
                (
                    u32::try_from(index).expect("content catalog is bounded below u32::MAX"),
                    plugin,
                )
            })
    }

    /// Qualify a source-local ID without exposing a load-order-dependent FormID.
    ///
    /// This proves that the source is loaded and that the local ID fits its
    /// slot class. It does not claim that a record with that ID exists; record
    /// metadata/query services perform that stronger lookup.
    pub fn qualify_form(&self, plugin_name: &str, local: u32) -> Option<FormRef> {
        if local == 0 {
            return None;
        }
        let (_, plugin) = self.find(plugin_name)?;
        let valid = match plugin.kind {
            PluginKind::Regular => local <= 0x00ff_ffff,
            PluginKind::Light => local <= 0x0000_0fff,
        };
        valid.then(|| FormRef::new(plugin.source, local))
    }

    pub fn iter(&self) -> impl Iterator<Item = &PluginInfo> {
        self.0.iter()
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ContentCatalogError {
    #[error("invalid plugin basename {0:?}")]
    InvalidPluginName(String),
    #[error("loaded plugin count exceeds {maximum}")]
    PluginBudgetExceeded { maximum: usize },
    #[error("loaded plugin basename is duplicated case-insensitively: {0}")]
    DuplicatePluginName(String),
    #[error("loaded plugins repeat stable source identity {0:02x?}")]
    DuplicateSource([u8; 16]),
    #[error("dependency lists must be parallel with loaded plugins")]
    DependencyShapeMismatch,
    #[error("plugin {plugin} has invalid forward or self dependency {dependency}")]
    InvalidDependency { plugin: u32, dependency: u32 },
    #[error("plugin {plugin} repeats dependency {dependency}")]
    DuplicateDependency { plugin: u32, dependency: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin(name: &str, source: u128, kind: PluginKind) -> PluginInfo {
        PluginInfo::new(name, source.to_be_bytes(), kind).unwrap()
    }

    #[test]
    fn catalog_lookup_is_case_insensitive_but_identity_is_portable() {
        let catalog = ContentCatalog::new(vec![
            plugin("Skyrim.esm", 1, PluginKind::Regular),
            plugin("Update.esm", 2, PluginKind::Regular),
        ])
        .unwrap();
        assert_eq!(catalog.find("SKYRIM.ESM").unwrap().0, 0);
        assert_eq!(
            catalog.qualify_form("update.ESM", 0x1234),
            Some(FormRef::new(2_u128.to_be_bytes(), 0x1234))
        );
        assert_eq!(catalog.qualify_form("missing.esm", 1), None);
        assert_eq!(catalog.qualify_form("Skyrim.esm", 0), None);
    }

    #[test]
    fn light_and_regular_local_id_ranges_are_enforced() {
        let catalog = ContentCatalog::new(vec![
            plugin("Full.esm", 1, PluginKind::Regular),
            plugin("Light.esl", 2, PluginKind::Light),
        ])
        .unwrap();
        assert!(catalog.qualify_form("Full.esm", 0x00ff_ffff).is_some());
        assert!(catalog.qualify_form("Full.esm", 0x0100_0000).is_none());
        assert!(catalog.qualify_form("Light.esl", 0x0fff).is_some());
        assert!(catalog.qualify_form("Light.esl", 0x1000).is_none());
    }

    #[test]
    fn catalog_rejects_ambiguous_or_unsafe_sources() {
        assert!(matches!(
            ContentCatalog::new(vec![
                plugin("Example.esm", 1, PluginKind::Regular),
                plugin("example.ESM", 2, PluginKind::Regular),
            ]),
            Err(ContentCatalogError::DuplicatePluginName(_))
        ));
        assert!(PluginInfo::new("../escape.esm", [0; 16], PluginKind::Regular).is_err());
    }

    #[test]
    fn dependencies_are_ordered_bounded_edges_to_earlier_plugins() {
        let catalog = ContentCatalog::new_with_dependencies(
            vec![
                plugin("Skyrim.esm", 1, PluginKind::Regular),
                plugin("Update.esm", 2, PluginKind::Regular),
                plugin("Example.esp", 3, PluginKind::Regular),
            ],
            vec![vec![], vec![0], vec![0, 1]],
        )
        .unwrap();
        assert_eq!(catalog.plugin(2).unwrap().dependencies(), &[0, 1]);
        assert_eq!(catalog.dependency(2, 0), Some(0));
        assert_eq!(catalog.dependency(2, 2), None);

        assert!(matches!(
            ContentCatalog::new_with_dependencies(
                vec![
                    plugin("Skyrim.esm", 1, PluginKind::Regular),
                    plugin("Update.esm", 2, PluginKind::Regular),
                ],
                vec![vec![], vec![1]],
            ),
            Err(ContentCatalogError::InvalidDependency { .. })
        ));
    }
}
