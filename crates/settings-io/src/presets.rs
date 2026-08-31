//! Named bundles of setting values — Low / Medium / High / Ultra.
//!
//! **Scope, stated plainly.** A preset can only move settings that are actually
//! registered, and today the registry holds exactly one graphics-quality knob:
//! `render.upscaler`. So the shipped presets vary the upscaler and nothing
//! else. The mechanism is general — a preset is a list of `(id, value)` pairs,
//! and adding shadow, texture, or draw-distance settings to the registry
//! extends the presets by editing `assets/graphics_presets.toml`, with no code
//! change here — but it would be dishonest to call the current file a full
//! quality ladder.
//!
//! Values are applied through [`SettingsRegistry::set`], so a preset is bound
//! by exactly the same validation as a hand edit: a preset cannot push a slider
//! out of range or name a choice that does not exist.

use std::collections::BTreeMap;
use std::path::Path;

use byroredux_core::settings::{SettingValue, SettingsRegistry};
use serde::{Deserialize, Serialize};

/// Where the shipped preset file lives, relative to the working directory.
pub const DEFAULT_PRESETS_PATH: &str = "assets/graphics_presets.toml";

/// The label shown when the current values match no preset.
pub const CUSTOM_LABEL: &str = "Custom";

/// One named bundle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    /// Display name, e.g. `"Medium"`.
    pub name: String,
    /// One sentence describing the trade.
    #[serde(default)]
    pub description: String,
    /// Sort order in the UI, ascending.
    #[serde(default)]
    pub rank: u32,
    /// `setting id → value`. Strings, numbers, and booleans map onto
    /// `SettingValue::Choice`, `::Number`, and `::Bool` respectively.
    #[serde(default)]
    pub values: BTreeMap<String, toml::Value>,
}

/// The shipped preset set, keyed by a stable slug (`low`, `medium`, …).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PresetFile {
    #[serde(default)]
    pub presets: BTreeMap<String, Preset>,
}

/// What happened when a preset was applied.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApplyReport {
    /// Settings whose value changed.
    pub changed: Vec<String>,
    /// Settings the preset named that the registry does not have. Expected
    /// rather than exceptional: a preset file may describe knobs a future
    /// engine registers, and skipping them individually keeps the rest working.
    pub unknown: Vec<String>,
    /// Settings the registry rejected, with the reason.
    pub rejected: Vec<(String, String)>,
}

impl PresetFile {
    /// Read a preset file. A missing file is an empty set, not an error.
    pub fn load(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        match toml::from_str(&text) {
            Ok(parsed) => parsed,
            Err(error) => {
                log::warn!("presets: could not parse {}: {error}", path.display());
                Self::default()
            }
        }
    }

    /// The shipped file, tried relative to the working directory and then to
    /// the running executable — the same two-step the profile loader uses, so a
    /// release build beside its `assets/` works without a working-directory
    /// convention.
    pub fn load_default() -> Self {
        let cwd = Self::load(DEFAULT_PRESETS_PATH);
        if !cwd.presets.is_empty() {
            return cwd;
        }
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join(DEFAULT_PRESETS_PATH)))
            .map(Self::load)
            .unwrap_or_default()
    }

    /// Presets in display order.
    pub fn ordered(&self) -> Vec<(&String, &Preset)> {
        let mut all: Vec<(&String, &Preset)> = self.presets.iter().collect();
        all.sort_by_key(|(slug, preset)| (preset.rank, (*slug).clone()));
        all
    }

    /// The preset whose every value the registry currently matches, if any.
    ///
    /// This is what drives the `Custom` label: editing one control after
    /// choosing a preset must stop claiming the preset, without discarding the
    /// values the user now has.
    pub fn active<'a>(&'a self, registry: &SettingsRegistry) -> Option<(&'a str, &'a Preset)> {
        self.ordered().into_iter().find_map(|(slug, preset)| {
            let matches = preset.values.iter().all(|(id, raw)| {
                match (registry.get(id), decode(raw)) {
                    (Some(entry), Some(value)) => entry.value == value,
                    // A value the registry does not have cannot disagree, so it
                    // must not veto the match — otherwise a preset naming one
                    // future knob could never be reported as active.
                    _ => true,
                }
            });
            matches.then_some((slug.as_str(), preset))
        })
    }
}

/// Apply a preset's values to the registry.
pub fn apply(preset: &Preset, registry: &mut SettingsRegistry) -> ApplyReport {
    let mut report = ApplyReport::default();
    for (id, raw) in &preset.values {
        let Some(value) = decode(raw) else {
            report
                .rejected
                .push((id.clone(), format!("unsupported value {raw}")));
            continue;
        };
        if registry.get(id).is_none() {
            report.unknown.push(id.clone());
            continue;
        }
        match registry.set(id, value) {
            Ok(true) => report.changed.push(id.clone()),
            Ok(false) => {}
            Err(error) => report.rejected.push((id.clone(), error.to_string())),
        }
    }
    report
}

/// TOML scalar → [`SettingValue`].
fn decode(raw: &toml::Value) -> Option<SettingValue> {
    match raw {
        toml::Value::String(value) => Some(SettingValue::Choice(value.clone())),
        toml::Value::Boolean(value) => Some(SettingValue::Bool(*value)),
        toml::Value::Float(value) => Some(SettingValue::Number(*value as f32)),
        toml::Value::Integer(value) => Some(SettingValue::Number(*value as f32)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::settings::{SettingChoice, SettingEntry};

    fn registry() -> SettingsRegistry {
        let mut registry = SettingsRegistry::default();
        registry
            .register(SettingEntry::choice(
                "render.upscaler",
                "Rendering",
                "Upscaler",
                "",
                "fsr3/quality",
                vec![
                    SettingChoice::new("taa", "TAA"),
                    SettingChoice::new("fsr3/quality", "Quality"),
                    SettingChoice::new("fsr3/performance", "Performance"),
                ],
            ))
            .unwrap();
        registry
            .register(SettingEntry::slider(
                "gameplay.field_of_view",
                "Gameplay",
                "FOV",
                "",
                45.0,
                45.0,
                110.0,
                1.0,
                "°",
            ))
            .unwrap();
        registry
    }

    fn preset(pairs: &[(&str, toml::Value)]) -> Preset {
        Preset {
            name: "Test".into(),
            description: String::new(),
            rank: 0,
            values: pairs
                .iter()
                .map(|(id, value)| ((*id).to_owned(), value.clone()))
                .collect(),
        }
    }

    #[test]
    fn applying_a_preset_changes_the_named_settings() {
        let mut registry = registry();
        let report = apply(
            &preset(&[("render.upscaler", toml::Value::String("taa".into()))]),
            &mut registry,
        );
        assert_eq!(report.changed, ["render.upscaler"]);
        assert_eq!(
            registry.get("render.upscaler").unwrap().value,
            SettingValue::Choice("taa".into())
        );
    }

    /// A preset is bound by the same validation as a hand edit — it cannot push
    /// a slider out of range or name a choice that does not exist. Without
    /// this, a bad preset file would be a way to smuggle invalid values past
    /// the registry.
    #[test]
    fn a_preset_cannot_smuggle_an_invalid_value_past_the_registry() {
        let mut registry = registry();
        let report = apply(
            &preset(&[
                ("gameplay.field_of_view", toml::Value::Float(400.0)),
                ("render.upscaler", toml::Value::String("dlss".into())),
            ]),
            &mut registry,
        );
        assert!(report.changed.is_empty(), "{:?}", report.changed);
        assert_eq!(report.rejected.len(), 2, "{report:?}");
        assert_eq!(
            registry.get("gameplay.field_of_view").unwrap().value,
            SettingValue::Number(45.0)
        );
    }

    /// A preset may name knobs a future engine registers; those are skipped
    /// individually so the rest of the preset still applies.
    #[test]
    fn unknown_settings_are_skipped_not_fatal() {
        let mut registry = registry();
        let report = apply(
            &preset(&[
                ("render.shadow_distance", toml::Value::Integer(4000)),
                ("render.upscaler", toml::Value::String("taa".into())),
            ]),
            &mut registry,
        );
        assert_eq!(report.unknown, ["render.shadow_distance"]);
        assert_eq!(report.changed, ["render.upscaler"]);
    }

    /// Editing one control after choosing a preset must stop claiming the
    /// preset — that is the whole `Custom` behaviour.
    #[test]
    fn a_hand_edit_after_a_preset_reads_as_custom() {
        let mut registry = registry();
        let file = PresetFile {
            presets: BTreeMap::from([(
                "low".to_owned(),
                preset(&[(
                    "render.upscaler",
                    toml::Value::String("fsr3/performance".into()),
                )]),
            )]),
        };
        apply(&file.presets["low"], &mut registry);
        assert_eq!(file.active(&registry).map(|(slug, _)| slug), Some("low"));

        registry
            .set("render.upscaler", SettingValue::Choice("taa".into()))
            .unwrap();
        assert_eq!(file.active(&registry), None);
    }

    /// The shipped file must parse, name only real settings, and hold values
    /// the registry accepts — a preset that silently rejects everything would
    /// look like it worked.
    #[test]
    fn the_shipped_presets_apply_cleanly_to_the_real_builtin_registry() {
        let path = std::path::Path::new("../../assets/graphics_presets.toml");
        if !path.exists() {
            return; // not running from the repo
        }
        let file = PresetFile::load(path);
        assert!(!file.presets.is_empty(), "shipped preset file is empty");

        for (slug, preset) in file.ordered() {
            let mut registry = SettingsRegistry::default();
            byroredux_core::settings::builtin::register_builtin_settings(&mut registry).unwrap();
            let report = apply(preset, &mut registry);
            assert!(
                report.rejected.is_empty(),
                "preset `{slug}` has values the registry refuses: {:?}",
                report.rejected
            );
            assert!(
                report.unknown.is_empty(),
                "preset `{slug}` names settings that do not exist: {:?}",
                report.unknown
            );
            assert!(
                !preset.values.is_empty(),
                "preset `{slug}` sets nothing at all"
            );
        }
    }
}
