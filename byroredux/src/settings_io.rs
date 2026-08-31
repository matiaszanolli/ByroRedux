//! Settings persistence, re-exported from `byroredux-settings-io`.
//!
//! Moved out of the binary so the launcher writes the same file the engine
//! reads before `VulkanContext` is created (`docs/engine/launcher.md` §4).
//! This shim keeps `crate::settings_io::…` working for every call site.

pub(crate) use byroredux_settings_io::{load, save, SettingsPersistence, SETTINGS_PATH_ENV};
