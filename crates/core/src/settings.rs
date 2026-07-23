//! Game-independent runtime settings registry.
//!
//! The registry owns setting metadata and current values, while presentation
//! layers (the on-screen debug UI today, native game menus later) render a
//! cloned snapshot and submit [`SettingChange`] values back to it. Keeping the
//! model in `core` prevents renderer, input, audio, and gameplay settings from
//! depending on a particular menu implementation.

use crate::ecs::Resource;
use std::collections::{BTreeMap, BTreeSet};

/// Current or default value for one setting.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingValue {
    Bool(bool),
    Number(f32),
    Choice(String),
}

/// One selectable value presented by a [`SettingControl::Choice`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingChoice {
    pub value: String,
    pub label: String,
}

impl SettingChoice {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

/// Widget contract for a setting. Presentation layers remain free to choose
/// their visual style while respecting these validation bounds.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingControl {
    Toggle,
    Slider {
        min: f32,
        max: f32,
        step: f32,
        unit: String,
    },
    Choice {
        options: Vec<SettingChoice>,
    },
}

/// Metadata and state for one registered setting.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingEntry {
    pub id: String,
    pub section: String,
    pub label: String,
    pub description: String,
    pub value: SettingValue,
    pub default: SettingValue,
    pub control: SettingControl,
    pub restart_required: bool,
}

impl SettingEntry {
    pub fn toggle(
        id: impl Into<String>,
        section: impl Into<String>,
        label: impl Into<String>,
        description: impl Into<String>,
        default: bool,
    ) -> Self {
        Self {
            id: id.into(),
            section: section.into(),
            label: label.into(),
            description: description.into(),
            value: SettingValue::Bool(default),
            default: SettingValue::Bool(default),
            control: SettingControl::Toggle,
            restart_required: false,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn slider(
        id: impl Into<String>,
        section: impl Into<String>,
        label: impl Into<String>,
        description: impl Into<String>,
        default: f32,
        min: f32,
        max: f32,
        step: f32,
        unit: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            section: section.into(),
            label: label.into(),
            description: description.into(),
            value: SettingValue::Number(default),
            default: SettingValue::Number(default),
            control: SettingControl::Slider {
                min,
                max,
                step,
                unit: unit.into(),
            },
            restart_required: false,
        }
    }

    pub fn choice(
        id: impl Into<String>,
        section: impl Into<String>,
        label: impl Into<String>,
        description: impl Into<String>,
        default: impl Into<String>,
        options: Vec<SettingChoice>,
    ) -> Self {
        let default = SettingValue::Choice(default.into());
        Self {
            id: id.into(),
            section: section.into(),
            label: label.into(),
            description: description.into(),
            value: default.clone(),
            default,
            control: SettingControl::Choice { options },
            restart_required: false,
        }
    }

    pub fn requiring_restart(mut self) -> Self {
        self.restart_required = true;
        self
    }
}

/// A value change emitted by any settings presentation layer.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingChange {
    pub id: String,
    pub value: SettingValue,
}

impl SettingChange {
    pub fn new(id: impl Into<String>, value: SettingValue) -> Self {
        Self {
            id: id.into(),
            value,
        }
    }
}

/// Validation failures from registration or mutation.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum SettingsError {
    #[error("setting id, section, and label must be non-empty")]
    MissingIdentity,
    #[error("setting '{0}' is already registered")]
    Duplicate(String),
    #[error("setting '{0}' is not registered")]
    Unknown(String),
    #[error("setting '{id}' expects {expected}, received {received}")]
    TypeMismatch {
        id: String,
        expected: &'static str,
        received: &'static str,
    },
    #[error("setting '{id}' has invalid slider bounds")]
    InvalidSlider { id: String },
    #[error("setting '{id}' value {value} is outside [{min}, {max}]")]
    OutOfRange {
        id: String,
        value: f32,
        min: f32,
        max: f32,
    },
    #[error("setting '{id}' has no choices")]
    EmptyChoices { id: String },
    #[error("setting '{id}' contains duplicate or empty choice values")]
    InvalidChoices { id: String },
    #[error("setting '{id}' does not allow choice '{value}'")]
    UnknownChoice { id: String, value: String },
}

/// Stable, deterministic collection of universal runtime settings.
#[derive(Debug, Default, Clone)]
pub struct SettingsRegistry {
    entries: BTreeMap<String, SettingEntry>,
}

impl Resource for SettingsRegistry {}

impl SettingsRegistry {
    pub fn register(&mut self, entry: SettingEntry) -> Result<(), SettingsError> {
        validate_entry(&entry)?;
        if self.entries.contains_key(&entry.id) {
            return Err(SettingsError::Duplicate(entry.id));
        }
        self.entries.insert(entry.id.clone(), entry);
        Ok(())
    }

    pub fn contains(&self, id: &str) -> bool {
        self.entries.contains_key(id)
    }

    pub fn get(&self, id: &str) -> Option<&SettingEntry> {
        self.entries.get(id)
    }

    pub fn entries(&self) -> impl ExactSizeIterator<Item = &SettingEntry> {
        self.entries.values()
    }

    /// Set a validated value. Returns whether the value actually changed.
    pub fn set(&mut self, id: &str, value: SettingValue) -> Result<bool, SettingsError> {
        let entry = self
            .entries
            .get_mut(id)
            .ok_or_else(|| SettingsError::Unknown(id.to_owned()))?;
        validate_value(&entry.id, &entry.control, &value)?;
        let changed = entry.value != value;
        entry.value = value;
        Ok(changed)
    }

    pub fn reset(&mut self, id: &str) -> Result<bool, SettingsError> {
        let default = self
            .entries
            .get(id)
            .ok_or_else(|| SettingsError::Unknown(id.to_owned()))?
            .default
            .clone();
        self.set(id, default)
    }
}

fn validate_entry(entry: &SettingEntry) -> Result<(), SettingsError> {
    if entry.id.trim().is_empty()
        || entry.section.trim().is_empty()
        || entry.label.trim().is_empty()
    {
        return Err(SettingsError::MissingIdentity);
    }
    validate_control(&entry.id, &entry.control)?;
    validate_value(&entry.id, &entry.control, &entry.default)?;
    validate_value(&entry.id, &entry.control, &entry.value)
}

fn validate_control(id: &str, control: &SettingControl) -> Result<(), SettingsError> {
    match control {
        SettingControl::Toggle => Ok(()),
        SettingControl::Slider { min, max, step, .. } => {
            if min.is_finite() && max.is_finite() && step.is_finite() && min < max && *step > 0.0 {
                Ok(())
            } else {
                Err(SettingsError::InvalidSlider { id: id.to_owned() })
            }
        }
        SettingControl::Choice { options } => {
            if options.is_empty() {
                return Err(SettingsError::EmptyChoices { id: id.to_owned() });
            }
            let mut values = BTreeSet::new();
            if options.iter().any(|option| {
                option.value.trim().is_empty() || !values.insert(option.value.as_str())
            }) {
                return Err(SettingsError::InvalidChoices { id: id.to_owned() });
            }
            Ok(())
        }
    }
}

fn validate_value(
    id: &str,
    control: &SettingControl,
    value: &SettingValue,
) -> Result<(), SettingsError> {
    match (control, value) {
        (SettingControl::Toggle, SettingValue::Bool(_)) => Ok(()),
        (SettingControl::Slider { min, max, .. }, SettingValue::Number(number)) => {
            if number.is_finite() && number >= min && number <= max {
                Ok(())
            } else {
                Err(SettingsError::OutOfRange {
                    id: id.to_owned(),
                    value: *number,
                    min: *min,
                    max: *max,
                })
            }
        }
        (SettingControl::Choice { options }, SettingValue::Choice(selected)) => {
            if options.iter().any(|option| option.value == *selected) {
                Ok(())
            } else {
                Err(SettingsError::UnknownChoice {
                    id: id.to_owned(),
                    value: selected.clone(),
                })
            }
        }
        (SettingControl::Toggle, other) => Err(type_mismatch(id, "boolean", other)),
        (SettingControl::Slider { .. }, other) => Err(type_mismatch(id, "number", other)),
        (SettingControl::Choice { .. }, other) => Err(type_mismatch(id, "choice", other)),
    }
}

fn type_mismatch(id: &str, expected: &'static str, value: &SettingValue) -> SettingsError {
    let received = match value {
        SettingValue::Bool(_) => "boolean",
        SettingValue::Number(_) => "number",
        SettingValue::Choice(_) => "choice",
    };
    SettingsError::TypeMismatch {
        id: id.to_owned(),
        expected,
        received,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_updates_and_resets_a_validated_slider() {
        let mut registry = SettingsRegistry::default();
        registry
            .register(SettingEntry::slider(
                "interface.scale",
                "Interface",
                "Scale",
                "Overlay scale",
                1.0,
                0.75,
                2.0,
                0.05,
                "×",
            ))
            .unwrap();

        assert!(registry
            .set("interface.scale", SettingValue::Number(1.25))
            .unwrap());
        assert_eq!(
            registry.get("interface.scale").unwrap().value,
            SettingValue::Number(1.25)
        );
        assert!(registry.reset("interface.scale").unwrap());
        assert_eq!(
            registry.get("interface.scale").unwrap().value,
            SettingValue::Number(1.0)
        );
    }

    #[test]
    fn registry_rejects_wrong_type_and_out_of_range_values() {
        let mut registry = SettingsRegistry::default();
        registry
            .register(SettingEntry::slider(
                "audio.volume",
                "Audio",
                "Volume",
                "Master volume",
                1.0,
                0.0,
                1.0,
                0.05,
                "",
            ))
            .unwrap();

        assert!(matches!(
            registry.set("audio.volume", SettingValue::Bool(false)),
            Err(SettingsError::TypeMismatch { .. })
        ));
        assert!(matches!(
            registry.set("audio.volume", SettingValue::Number(1.5)),
            Err(SettingsError::OutOfRange { .. })
        ));
    }

    #[test]
    fn choices_require_unique_options_and_a_known_default() {
        let duplicate = SettingEntry::choice(
            "graphics.quality",
            "Graphics",
            "Quality",
            "Quality preset",
            "high",
            vec![
                SettingChoice::new("high", "High"),
                SettingChoice::new("high", "Also high"),
            ],
        );
        assert!(matches!(
            SettingsRegistry::default().register(duplicate),
            Err(SettingsError::InvalidChoices { .. })
        ));

        let unknown_default = SettingEntry::choice(
            "graphics.quality",
            "Graphics",
            "Quality",
            "Quality preset",
            "ultra",
            vec![SettingChoice::new("high", "High")],
        );
        assert!(matches!(
            SettingsRegistry::default().register(unknown_default),
            Err(SettingsError::UnknownChoice { .. })
        ));
    }
}
