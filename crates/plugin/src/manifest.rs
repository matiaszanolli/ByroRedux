//! Plugin manifest: identity, version, and dependency declarations.
//!
//! Redux-native plugins declare their identity and dependencies in a
//! `plugin.toml` file. Dependencies reference plugins by UUID — not by
//! filename or load order — so no external sorting tool is needed.

use gamebyro_core::form_id::PluginId;
use serde::Deserialize;

/// A loaded plugin's metadata, parsed from `plugin.toml`.
#[derive(Debug, Clone)]
pub struct PluginManifest {
    pub id: PluginId,
    pub name: String,
    pub version: semver::Version,
    pub dependencies: Vec<PluginId>,
}

impl PluginManifest {
    /// Parse a manifest from a TOML string.
    ///
    /// ```
    /// # use gamebyro_plugin::manifest::PluginManifest;
    /// let toml = r#"
    /// [plugin]
    /// uuid = "550e8400-e29b-41d4-a716-446655440000"
    /// name = "TestMod"
    /// version = "1.0.0"
    ///
    /// [[dependencies]]
    /// uuid = "12345678-1234-1234-1234-123456789abc"
    /// name = "BaseMaster.esm"
    /// "#;
    /// let manifest = PluginManifest::from_toml(toml).unwrap();
    /// assert_eq!(manifest.name, "TestMod");
    /// assert_eq!(manifest.dependencies.len(), 1);
    /// ```
    pub fn from_toml(src: &str) -> Result<Self, toml::de::Error> {
        let raw: RawManifest = toml::from_str(src)?;
        Ok(Self {
            id: PluginId::from_uuid(raw.plugin.uuid),
            name: raw.plugin.name,
            version: raw.plugin.version,
            dependencies: raw
                .dependencies
                .unwrap_or_default()
                .into_iter()
                .map(|d| PluginId::from_uuid(d.uuid))
                .collect(),
        })
    }
}

// ── Raw serde types (private) ───────────────────────────────────────────

#[derive(Deserialize)]
struct RawManifest {
    plugin: RawPlugin,
    #[serde(rename = "dependencies")]
    dependencies: Option<Vec<RawDependency>>,
}

#[derive(Deserialize)]
struct RawPlugin {
    uuid: uuid::Uuid,
    name: String,
    version: semver::Version,
}

#[derive(Deserialize)]
struct RawDependency {
    uuid: uuid::Uuid,
    #[allow(dead_code)]
    name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_manifest() {
        let toml = r#"
[plugin]
uuid = "550e8400-e29b-41d4-a716-446655440000"
name = "TestMod"
version = "1.2.3"

[[dependencies]]
uuid = "12345678-1234-1234-1234-123456789abc"
name = "BaseMaster.esm"

[[dependencies]]
uuid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
"#;
        let manifest = PluginManifest::from_toml(toml).unwrap();
        assert_eq!(manifest.name, "TestMod");
        assert_eq!(manifest.version, semver::Version::new(1, 2, 3));
        assert_eq!(manifest.dependencies.len(), 2);

        // PluginId round-trips through UUID
        let expected_id = PluginId::from_uuid("550e8400-e29b-41d4-a716-446655440000".parse().unwrap());
        assert_eq!(manifest.id, expected_id);
    }

    #[test]
    fn parse_manifest_no_dependencies() {
        let toml = r#"
[plugin]
uuid = "550e8400-e29b-41d4-a716-446655440000"
name = "StandaloneMod"
version = "0.1.0"
"#;
        let manifest = PluginManifest::from_toml(toml).unwrap();
        assert_eq!(manifest.name, "StandaloneMod");
        assert!(manifest.dependencies.is_empty());
    }

    #[test]
    fn parse_invalid_toml_returns_error() {
        let bad = "this is not toml {{{";
        assert!(PluginManifest::from_toml(bad).is_err());
    }
}
