//! Persistent user settings.
//!
//! The universal registry remains the source of truth for metadata and
//! validation. This crate persists only `(id, value)` pairs, then overlays them
//! onto freshly registered defaults at boot. Unknown or stale entries are
//! ignored individually so adding or removing a setting never makes the whole
//! file unreadable.
//!
//! Its own crate, not a module in the engine binary, because the launcher
//! writes the same file the engine reads before `VulkanContext` is created —
//! which is the mechanism that lets a launcher steer renderer setup without the
//! engine knowing a launcher exists (`docs/engine/launcher.md` §4).

pub mod presets;

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use byroredux_core::ecs::Resource;
use byroredux_core::settings::{SettingValue, SettingsRegistry};
use serde::{Deserialize, Serialize};

const SETTINGS_VERSION: u32 = 1;
/// Environment override for the settings-registry path. Also honoured by
/// the `--boot` launcher handoff, which points at it rather than passing a
/// flag, so both routes resolve through one name.
pub const SETTINGS_PATH_ENV: &str = "BYROREDUX_SETTINGS_PATH";

/// Location of the user settings file. Kept as a resource so every native
/// settings frontend writes through the same path.
#[derive(Debug, Clone)]
pub struct SettingsPersistence {
    path: PathBuf,
    /// Ids whose live registry value is a one-launch override (#4974). `save`
    /// leaves their stored value untouched until [`Self::unpin_stored`].
    pinned: BTreeSet<String>,
}

impl Resource for SettingsPersistence {}

impl SettingsPersistence {
    pub fn discover() -> Self {
        Self {
            path: discover_settings_path(),
            pinned: BTreeSet::new(),
        }
    }

    /// Persistence at an explicit path, e.g. a test's temporary directory.
    pub fn at(path: PathBuf) -> Self {
        Self {
            path,
            pinned: BTreeSet::new(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Keep `id`'s stored value on disk whatever the registry holds, for a
    /// value seeded for this launch only — an explicit CLI flag, say, that the
    /// registry must show but that must not become the persisted default the
    /// next time an unrelated setting is saved (#4974).
    pub fn pin_stored(&mut self, id: impl Into<String>) {
        self.pinned.insert(id.into());
    }

    /// Persist `id` from the registry again — the user made an explicit
    /// in-session choice that supersedes the launch override. Returns whether
    /// `id` was pinned.
    pub fn unpin_stored(&mut self, id: &str) -> bool {
        self.pinned.remove(id)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct StoredSettings {
    version: u32,
    #[serde(default)]
    settings: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LoadReport {
    pub applied: usize,
    pub ignored: usize,
}

pub fn load(registry: &mut SettingsRegistry, persistence: &SettingsPersistence) {
    match load_from_path(registry, persistence.path()) {
        Ok(report) if report.applied > 0 || report.ignored > 0 => log::info!(
            "settings: loaded {} value(s), ignored {} from {}",
            report.applied,
            report.ignored,
            persistence.path().display()
        ),
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => log::warn!(
            "settings: could not load {}: {error}",
            persistence.path().display()
        ),
    }
}

pub fn save(registry: &SettingsRegistry, persistence: &SettingsPersistence) {
    if let Err(error) = save_to_path(registry, persistence.path(), &persistence.pinned) {
        log::warn!(
            "settings: could not save {}: {error}",
            persistence.path().display()
        );
    }
}

fn load_from_path(registry: &mut SettingsRegistry, path: &Path) -> std::io::Result<LoadReport> {
    let source = fs::read_to_string(path)?;
    let stored: StoredSettings = toml::from_str(&source).map_err(std::io::Error::other)?;
    if stored.version != SETTINGS_VERSION {
        log::warn!(
            "settings: file {} uses version {}, expected {}; attempting compatible values",
            path.display(),
            stored.version,
            SETTINGS_VERSION
        );
    }

    let mut report = LoadReport::default();
    for (id, stored_value) in stored.settings {
        let Some(entry) = registry.get(&id) else {
            report.ignored += 1;
            continue;
        };
        let Some(value) = decode_value(&entry.default, stored_value) else {
            report.ignored += 1;
            continue;
        };
        match registry.set(&id, value) {
            Ok(_) => report.applied += 1,
            Err(error) => {
                report.ignored += 1;
                log::warn!("settings: ignored '{id}': {error}");
            }
        }
    }
    Ok(report)
}

/// Write the registry's values, **preserving stored keys it does not know**.
///
/// The preservation is not defensive coding; it is required by the launcher.
/// Two front ends now write this file with different registries: the engine
/// registers the built-in settings *and* the input bindings, while the launcher
/// registers only the built-ins (key rebinding is not a launcher feature). A
/// plain replace would therefore mean that opening the launcher once silently
/// erased every key the player had rebound.
///
/// The same rule protects the engine from itself: a subsystem that has not
/// registered yet at save time no longer costs the user its values.
///
/// #5144 — preservation only holds when the existing file parses. When it
/// does not, the save is **refused** (after copying the file aside as
/// `<name>.bad`); a version newer than this build's, or a read error other
/// than NotFound, is refused the same way. Nothing here may erase stored
/// keys it cannot read.
///
/// Pinned ids ([`SettingsPersistence::pin_stored`]) keep whatever the file
/// already holds for them, and stay absent if it holds nothing.
fn save_to_path(
    registry: &SettingsRegistry,
    path: &Path,
    pinned: &BTreeSet<String>,
) -> std::io::Result<()> {
    let mut settings: BTreeMap<String, toml::Value> = match fs::read_to_string(path) {
        // First run: nothing to preserve.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
        // #5144 — an unreadable file (permissions, or the path is not a
        // file) is not an invitation to erase it and write only the saving
        // registry's keys.
        Err(error) => return Err(error),
        Ok(existing) => match toml::from_str::<StoredSettings>(&existing) {
            // #5144 — a file this build cannot parse (hand-edit typo, torn
            // write, or a future shape) is copied aside and the save is
            // refused. The old shape substituted an empty map, so one save
            // from the launcher erased every stored key rebinding — and an
            // extension writing a setting did the same with no user action.
            Err(parse_error) => {
                let aside = bad_file_path(path);
                match fs::copy(path, &aside) {
                    Ok(_) => log::warn!(
                        "settings: {} could not be parsed ({parse_error}); kept it, copied it to {} for inspection, and refused to overwrite",
                        path.display(),
                        aside.display()
                    ),
                    Err(copy_error) => log::warn!(
                        "settings: {} could not be parsed ({parse_error}) and could not be copied aside ({copy_error}); refused to overwrite",
                        path.display()
                    ),
                }
                return Err(std::io::Error::other(format!(
                    "existing settings file could not be parsed: {parse_error}"
                )));
            }
            Ok(stored) => {
                // #5144 — never re-stamp a newer file down to this build's
                // version: that silently downgrades a future format.
                if stored.version > SETTINGS_VERSION {
                    log::warn!(
                        "settings: {} uses version {}, newer than the supported {}; refusing to overwrite",
                        path.display(),
                        stored.version,
                        SETTINGS_VERSION
                    );
                    return Err(std::io::Error::other(format!(
                        "settings file version {} is newer than supported {}",
                        stored.version, SETTINGS_VERSION
                    )));
                }
                stored.settings
            }
        },
    };
    for entry in registry.entries() {
        if pinned.contains(&entry.id) {
            continue;
        }
        settings.insert(entry.id.clone(), encode_value(&entry.value));
    }
    let source = toml::to_string_pretty(&StoredSettings {
        version: SETTINGS_VERSION,
        settings,
    })
    .map_err(std::io::Error::other)?;

    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let temp_path = byroredux_core::atomic_file::atomic_temp_path(path);
    // #3472 — was `fs::write` + `fs::rename` with none of the durability
    // steps: no fsync on the temp file, no read-back, no parent-directory
    // sync. A crash in the window between the rename hitting the directory
    // journal and the data reaching the platter left a zero-length or
    // truncated `settings.toml`. The loader degrades gracefully (a
    // `toml::from_str` failure is logged and skipped), so the cost was the
    // user's bindings and preferences rather than the session — but there is
    // no reason for two writers in one binary to have two different
    // durability contracts when one of them already implements the correct
    // dance. Shared with `crates/save/src/disk.rs`'s `write_slot` rather than
    // re-implemented, so the two cannot drift.
    //
    // #5143 — no non-atomic fallback on failure; see `BootRequest::save`
    // for why the old "Windows rename" branch guarded nothing real and
    // could copy the temp's partial bytes over the good file.
    byroredux_core::atomic_file::atomic_write(path, &temp_path, source.as_bytes())?;
    Ok(())
}

/// Where an unparseable settings file is copied aside before the save is
/// refused (#5144): `<name>.bad` beside it, so nothing is lost and the
/// reason is one `diff` away.
fn bad_file_path(path: &Path) -> PathBuf {
    let mut file_name = path
        .file_name()
        .map_or_else(|| OsString::from("settings.toml"), OsString::from);
    file_name.push(".bad");
    path.with_file_name(file_name)
}

fn encode_value(value: &SettingValue) -> toml::Value {
    match value {
        SettingValue::Bool(value) => toml::Value::Boolean(*value),
        SettingValue::Number(value) => toml::Value::Float(f64::from(*value)),
        SettingValue::Choice(value) => toml::Value::String(value.clone()),
    }
}

fn decode_value(expected: &SettingValue, value: toml::Value) -> Option<SettingValue> {
    match (expected, value) {
        (SettingValue::Bool(_), toml::Value::Boolean(value)) => Some(SettingValue::Bool(value)),
        (SettingValue::Number(_), toml::Value::Float(value)) => {
            Some(SettingValue::Number(value as f32))
        }
        (SettingValue::Number(_), toml::Value::Integer(value)) => {
            Some(SettingValue::Number(value as f32))
        }
        (SettingValue::Choice(_), toml::Value::String(value)) => Some(SettingValue::Choice(value)),
        _ => None,
    }
}

fn discover_settings_path() -> PathBuf {
    if let Some(path) = std::env::var_os(SETTINGS_PATH_ENV).filter(|path| !path.is_empty()) {
        return PathBuf::from(path);
    }

    #[cfg(target_os = "windows")]
    if let Some(root) = std::env::var_os("APPDATA") {
        return PathBuf::from(root).join("ByroRedux").join("settings.toml");
    }

    #[cfg(target_os = "macos")]
    if let Some(root) = std::env::var_os("HOME") {
        return PathBuf::from(root)
            .join("Library")
            .join("Application Support")
            .join("ByroRedux")
            .join("settings.toml");
    }

    if let Some(root) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(root).join("byroredux").join("settings.toml");
    }
    if let Some(root) = std::env::var_os("HOME") {
        return PathBuf::from(root)
            .join(".config")
            .join("byroredux")
            .join("settings.toml");
    }
    PathBuf::from("byroredux-settings.toml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::settings::{SettingChoice, SettingEntry};

    /// The launcher's registry is a strict subset of the engine's: it registers
    /// the built-in settings but not the input bindings, since key rebinding is
    /// not a launcher feature. A plain replace on save would therefore mean
    /// that opening the launcher once erased every key the player had rebound.
    #[test]
    fn saving_a_partial_registry_preserves_keys_it_does_not_know() {
        let dir = tempfile::tempdir().unwrap();
        let persistence = SettingsPersistence::at(dir.path().join("settings.toml"));

        // The engine writes both its own settings and an input binding.
        let mut engine = SettingsRegistry::default();
        engine
            .register(SettingEntry::slider(
                "gameplay.field_of_view",
                "Gameplay",
                "Field of view",
                "",
                45.0,
                45.0,
                110.0,
                1.0,
                "°",
            ))
            .unwrap();
        engine
            .register(SettingEntry::choice(
                "controls.bind.jump",
                "Controls",
                "Jump",
                "",
                "Space",
                vec![
                    SettingChoice::new("Space", "Space"),
                    SettingChoice::new("KeyE", "E"),
                ],
            ))
            .unwrap();
        engine
            .set("controls.bind.jump", SettingValue::Choice("KeyE".into()))
            .unwrap();
        save(&engine, &persistence);

        // The launcher knows only the built-in, changes it, and saves.
        let mut launcher = SettingsRegistry::default();
        launcher
            .register(SettingEntry::slider(
                "gameplay.field_of_view",
                "Gameplay",
                "Field of view",
                "",
                45.0,
                45.0,
                110.0,
                1.0,
                "°",
            ))
            .unwrap();
        load(&mut launcher, &persistence);
        launcher
            .set("gameplay.field_of_view", SettingValue::Number(90.0))
            .unwrap();
        save(&launcher, &persistence);

        // The engine starts: its rebound key must have survived, and the
        // launcher's edit must have taken effect.
        let mut reloaded = SettingsRegistry::default();
        reloaded
            .register(SettingEntry::slider(
                "gameplay.field_of_view",
                "Gameplay",
                "Field of view",
                "",
                45.0,
                45.0,
                110.0,
                1.0,
                "°",
            ))
            .unwrap();
        reloaded
            .register(SettingEntry::choice(
                "controls.bind.jump",
                "Controls",
                "Jump",
                "",
                "Space",
                vec![
                    SettingChoice::new("Space", "Space"),
                    SettingChoice::new("KeyE", "E"),
                ],
            ))
            .unwrap();
        load(&mut reloaded, &persistence);

        assert_eq!(
            reloaded.get("controls.bind.jump").unwrap().value,
            SettingValue::Choice("KeyE".into()),
            "the launcher erased a binding it does not know about"
        );
        assert_eq!(
            reloaded.get("gameplay.field_of_view").unwrap().value,
            SettingValue::Number(90.0)
        );
    }

    /// #4974 — a pinned (one-launch) value is shown by the registry but an
    /// unrelated save keeps the stored value; once unpinned it persists.
    #[test]
    fn pinned_launch_override_is_not_persisted_by_an_unrelated_save() {
        let dir = tempfile::tempdir().unwrap();
        let mut persistence = SettingsPersistence::at(dir.path().join("settings.toml"));
        let stored = |persistence: &SettingsPersistence| {
            let mut reloaded = registry();
            load(&mut reloaded, persistence);
            reloaded.get("render.upscaler").unwrap().value.clone()
        };

        let mut live = registry();
        live.set("render.upscaler", SettingValue::Choice("taa".into()))
            .unwrap();
        save(&live, &persistence);

        // Launch override: seeded into the registry, pinned on disk.
        persistence.pin_stored("render.upscaler");
        live.set("render.upscaler", SettingValue::Choice("fsr3/quality".into()))
            .unwrap();
        live.set("controls.sensitivity", SettingValue::Number(2.0))
            .unwrap();
        save(&live, &persistence);
        assert_eq!(stored(&persistence), SettingValue::Choice("taa".into()));
        let mut reloaded = registry();
        load(&mut reloaded, &persistence);
        assert_eq!(
            reloaded.get("controls.sensitivity").unwrap().value,
            SettingValue::Number(2.0),
            "the unrelated change itself must still persist"
        );

        // An explicit in-session choice releases the pin.
        assert!(persistence.unpin_stored("render.upscaler"));
        save(&live, &persistence);
        assert_eq!(
            stored(&persistence),
            SettingValue::Choice("fsr3/quality".into())
        );
    }

    fn registry() -> SettingsRegistry {
        let mut registry = SettingsRegistry::default();
        registry
            .register(SettingEntry::toggle(
                "interface.crosshair",
                "Interface",
                "Crosshair",
                "",
                true,
            ))
            .unwrap();
        registry
            .register(SettingEntry::slider(
                "controls.sensitivity",
                "Controls",
                "Sensitivity",
                "",
                1.0,
                0.1,
                4.0,
                0.1,
                "x",
            ))
            .unwrap();
        registry
            .register(SettingEntry::choice(
                "render.upscaler",
                "Rendering",
                "Upscaler",
                "",
                "taa",
                vec![
                    SettingChoice::new("taa", "TAA"),
                    SettingChoice::new("fsr3/quality", "FSR"),
                ],
            ))
            .unwrap();
        registry
    }

    #[test]
    fn settings_round_trip_through_toml() {
        let temp = tempfile::tempdir().unwrap();
        let persistence = SettingsPersistence::at(temp.path().join("nested/settings.toml"));
        let mut source = registry();
        source
            .set("interface.crosshair", SettingValue::Bool(false))
            .unwrap();
        source
            .set("controls.sensitivity", SettingValue::Number(2.25))
            .unwrap();
        source
            .set(
                "render.upscaler",
                SettingValue::Choice("fsr3/quality".to_owned()),
            )
            .unwrap();
        save_to_path(&source, persistence.path(), &BTreeSet::new()).unwrap();

        let mut restored = registry();
        let report = load_from_path(&mut restored, persistence.path()).unwrap();
        assert_eq!(
            report,
            LoadReport {
                applied: 3,
                ignored: 0
            }
        );
        assert_eq!(
            restored.get("interface.crosshair").unwrap().value,
            SettingValue::Bool(false)
        );
        assert_eq!(
            restored.get("controls.sensitivity").unwrap().value,
            SettingValue::Number(2.25)
        );
        assert_eq!(
            restored.get("render.upscaler").unwrap().value,
            SettingValue::Choice("fsr3/quality".to_owned())
        );
    }

    #[test]
    fn stale_and_invalid_values_do_not_block_valid_siblings() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("settings.toml");
        fs::write(
            &path,
            r#"
version = 1

[settings]
"interface.crosshair" = false
"controls.sensitivity" = 99.0
"removed.setting" = true
"#,
        )
        .unwrap();

        let mut restored = registry();
        let report = load_from_path(&mut restored, &path).unwrap();
        assert_eq!(
            report,
            LoadReport {
                applied: 1,
                ignored: 2
            }
        );
        assert_eq!(
            restored.get("interface.crosshair").unwrap().value,
            SettingValue::Bool(false)
        );
        assert_eq!(
            restored.get("controls.sensitivity").unwrap().value,
            SettingValue::Number(1.0)
        );
    }

    /// #5144 — the preservation contract only held while the existing file
    /// parsed; a parse failure used to substitute an empty map, so one save
    /// erased every stored key rebinding. The save is now refused after the
    /// file is copied aside, and the malformed bytes stay put.
    #[test]
    fn save_refuses_when_the_existing_file_does_not_parse() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        fs::write(&path, "not = = toml {{{").unwrap();

        let error = save_to_path(&registry(), &path, &BTreeSet::new()).unwrap_err();
        assert!(
            error.to_string().contains("could not be parsed"),
            "{error}"
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "not = = toml {{{",
            "no one may rewrite the malformed file"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("settings.toml.bad")).unwrap(),
            "not = = toml {{{",
            "the malformed file is preserved beside the live one for inspection"
        );
        // The public entry point logs and carries on; the file still wins.
        save(&registry(), &SettingsPersistence::at(path.clone()));
        assert_eq!(fs::read_to_string(&path).unwrap(), "not = = toml {{{");
    }

    /// #5144 — a parseable file from a newer build is not silently
    /// re-stamped down to this build's version.
    #[test]
    fn save_refuses_to_downgrade_a_newer_version_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        fs::write(&path, "version = 99\n[settings]\n").unwrap();

        let error = save_to_path(&registry(), &path, &BTreeSet::new()).unwrap_err();
        assert!(error.to_string().contains("newer"), "{error}");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "version = 99\n[settings]\n",
            "a newer-version file must not be re-stamped"
        );
        assert!(
            !dir.path().join("settings.toml.bad").exists(),
            "a parseable file is not 'bad'"
        );
    }

    /// #5144 — a read error other than NotFound (permissions, or the path
    /// is not a file) must refuse, not write a fresh file over it.
    #[test]
    fn save_refuses_when_the_existing_path_is_not_a_readable_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        fs::create_dir(&path).unwrap();

        assert!(save_to_path(&registry(), &path, &BTreeSet::new()).is_err());
        assert!(
            path.is_dir(),
            "nothing may replace an unreadable path"
        );
    }

    /// #5143 — the "Windows rename" fallback ran on any `atomic_write`
    /// error and could copy the temp's possibly-partial bytes over the good
    /// file. Its removal is pinned statically because the failure it
    /// mishandled (disk full, EIO on sync) is OS-level and cannot be
    /// fault-injected portably. Scans the production text only (#5164).
    #[test]
    fn save_has_no_nonatomic_clobber_fallback() {
        byroredux_core::atomic_file::assert_no_clobber_fallback(include_str!("lib.rs"));
    }
}
