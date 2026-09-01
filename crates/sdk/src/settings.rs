//! Stable, typed read-only engine settings exposed to sandboxed extensions.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::identity::{ExtensionId, SettingId};

pub const MAX_SETTINGS: usize = 512;
pub const MAX_EXTENSION_SETTINGS: usize = 64;
pub const MAX_SETTING_KEY_BYTES: usize = 128;
pub const MAX_SETTING_CHOICE_BYTES: usize = 256;
pub const MAX_SETTING_LABEL_BYTES: usize = 128;
pub const MAX_SETTING_DESCRIPTION_BYTES: usize = 512;
pub const MAX_SETTING_CHOICES: usize = 32;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SettingValue {
    Boolean(bool),
    Number(f32),
    Choice(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum SettingControlDeclaration {
    Toggle,
    Slider {
        min: f32,
        max: f32,
        step: f32,
        #[serde(default)]
        unit: String,
    },
    Choice {
        options: Vec<SettingChoiceDeclaration>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingChoiceDeclaration {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingDeclaration {
    pub id: SettingId,
    pub label: String,
    pub description: String,
    pub default: SettingValue,
    pub control: SettingControlDeclaration,
    #[serde(default)]
    pub restart_required: bool,
}

impl SettingDeclaration {
    pub fn qualified_name(&self, extension: &ExtensionId) -> String {
        format!("ext.{extension}.{}", self.id)
    }

    pub fn is_valid(&self) -> bool {
        let safe_text = |value: &str, max: usize| {
            !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
        };
        if !safe_text(&self.label, MAX_SETTING_LABEL_BYTES)
            || !safe_text(&self.description, MAX_SETTING_DESCRIPTION_BYTES)
        {
            return false;
        }
        match (&self.default, &self.control) {
            (SettingValue::Boolean(_), SettingControlDeclaration::Toggle) => true,
            (
                SettingValue::Number(value),
                SettingControlDeclaration::Slider {
                    min,
                    max,
                    step,
                    unit,
                },
            ) => {
                value.is_finite()
                    && min.is_finite()
                    && max.is_finite()
                    && step.is_finite()
                    && min < max
                    && *step > 0.0
                    && value >= min
                    && value <= max
                    && unit.len() <= MAX_SETTING_LABEL_BYTES
                    && !unit.chars().any(char::is_control)
            }
            (SettingValue::Choice(value), SettingControlDeclaration::Choice { options }) => {
                !options.is_empty()
                    && options.len() <= MAX_SETTING_CHOICES
                    && options.iter().all(|option| {
                        safe_text(&option.value, MAX_SETTING_CHOICE_BYTES)
                            && safe_text(&option.label, MAX_SETTING_LABEL_BYTES)
                    })
                    && options
                        .iter()
                        .filter(|option| option.value == *value)
                        .count()
                        == 1
                    && options
                        .iter()
                        .map(|option| &option.value)
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        == options.len()
            }
            _ => false,
        }
    }

    pub fn accepts(&self, value: &SettingValue) -> bool {
        match (value, &self.control) {
            (SettingValue::Boolean(_), SettingControlDeclaration::Toggle) => true,
            (SettingValue::Number(value), SettingControlDeclaration::Slider { min, max, .. }) => {
                value.is_finite() && value >= min && value <= max
            }
            (SettingValue::Choice(value), SettingControlDeclaration::Choice { options }) => {
                options.iter().any(|option| option.value == *value)
            }
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SettingWriteCommand {
    pub key: String,
    pub value: SettingValue,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SettingsSnapshot(BTreeMap<String, SettingValue>);

#[derive(Clone, Debug, Error, PartialEq)]
pub enum SettingsSnapshotError {
    #[error("settings snapshot exceeds {MAX_SETTINGS} entries")]
    TooManyEntries,
    #[error("invalid setting key {0:?}")]
    InvalidKey(String),
    #[error("setting {0:?} contains an invalid number or unbounded choice")]
    InvalidValue(String),
}

impl SettingsSnapshot {
    pub fn new(
        entries: impl IntoIterator<Item = (String, SettingValue)>,
    ) -> Result<Self, SettingsSnapshotError> {
        let entries = entries.into_iter().collect::<BTreeMap<_, _>>();
        if entries.len() > MAX_SETTINGS {
            return Err(SettingsSnapshotError::TooManyEntries);
        }
        for (key, value) in &entries {
            if key.is_empty()
                || key.len() > MAX_SETTING_KEY_BYTES
                || key.chars().any(char::is_control)
            {
                return Err(SettingsSnapshotError::InvalidKey(key.clone()));
            }
            let valid = match value {
                SettingValue::Boolean(_) => true,
                SettingValue::Number(value) => value.is_finite(),
                SettingValue::Choice(value) => {
                    value.len() <= MAX_SETTING_CHOICE_BYTES && !value.chars().any(char::is_control)
                }
            };
            if !valid {
                return Err(SettingsSnapshotError::InvalidValue(key.clone()));
            }
        }
        Ok(Self(entries))
    }

    pub fn get(&self, key: &str) -> Option<&SettingValue> {
        self.0.get(key)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_preserves_types_and_rejects_unsafe_values() {
        let settings = SettingsSnapshot::new([
            (
                "interface.crosshair".to_owned(),
                SettingValue::Boolean(true),
            ),
            ("gameplay.fov".to_owned(), SettingValue::Number(90.0)),
            (
                "render.upscaler".to_owned(),
                SettingValue::Choice("fsr3/quality".to_owned()),
            ),
        ])
        .unwrap();
        assert_eq!(settings.len(), 3);
        assert_eq!(
            settings.get("render.upscaler"),
            Some(&SettingValue::Choice("fsr3/quality".to_owned()))
        );
        assert!(
            SettingsSnapshot::new([("bad\nkey".to_owned(), SettingValue::Boolean(true))]).is_err()
        );
        assert!(
            SettingsSnapshot::new([("bad.number".to_owned(), SettingValue::Number(f32::NAN))])
                .is_err()
        );
    }

    #[test]
    fn declarations_are_namespaced_typed_and_bounded() {
        let declaration = SettingDeclaration {
            id: SettingId::new("difficulty").unwrap(),
            label: "Difficulty".to_owned(),
            description: "Extension difficulty multiplier".to_owned(),
            default: SettingValue::Number(1.0),
            control: SettingControlDeclaration::Slider {
                min: 0.5,
                max: 2.0,
                step: 0.1,
                unit: "x".to_owned(),
            },
            restart_required: false,
        };
        assert!(declaration.is_valid());
        assert_eq!(
            declaration.qualified_name(&ExtensionId::new("org.example.mod").unwrap()),
            "ext.org.example.mod.difficulty"
        );

        let mut invalid = declaration;
        invalid.default = SettingValue::Choice("hard".to_owned());
        assert!(!invalid.is_valid());
    }
}
