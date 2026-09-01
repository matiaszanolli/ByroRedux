//! Stable, typed read-only engine settings exposed to sandboxed extensions.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_SETTINGS: usize = 512;
pub const MAX_SETTING_KEY_BYTES: usize = 128;
pub const MAX_SETTING_CHOICE_BYTES: usize = 256;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SettingValue {
    Boolean(bool),
    Number(f32),
    Choice(String),
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
}
