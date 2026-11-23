//! MCM Helper's mod settings (`SettingStore.cpp`): typed INI keys a config's controls and a mod's
//! `GetModSetting…` / `SetModSetting…` read and write, named `key:Section`.
//!
//! A key's first letter is its type, as for the game's own settings: `b` bool, `i` int, `u` unsigned,
//! `f` float, `s` string, `r` / `a` colour. Names compare without case (SimpleIni's and the game's
//! fixed strings both do). Three layers, the later winning (`docs/engine/mcm-host.md` §5.1, owner's
//! decisions §6.4–6.5):
//!
//! 1. the mod's defaults, `MCM/Config/<mod>/settings.ini`;
//! 2. what an earlier MCM Helper install left in `MCM/Settings/<mod>.ini` (read, never written);
//! 3. the profile's own file, the only one written.
//!
//! As in MCM Helper, a setting none of the files declares cannot be set; a missing one reads as -1
//! (int, float), false or "".

use std::collections::{BTreeSet, HashMap};

/// The files the store reads and writes; the engine supplies the mod-root lookup and the profile
/// directory.
pub trait HelperFiles: Send + Sync {
    /// `MCM/Config/<mod>/config.json`.
    fn config(&self, mod_name: &str) -> Option<Vec<u8>>;
    /// `MCM/Config/<mod>/settings.ini`.
    fn defaults(&self, mod_name: &str) -> Option<Vec<u8>>;
    /// `MCM/Settings/<mod>.ini` an earlier MCM Helper install left (read only).
    fn legacy_settings(&self, mod_name: &str) -> Option<Vec<u8>>;
    /// The profile's own `<mod>.ini`.
    fn profile_settings(&self, mod_name: &str) -> Option<Vec<u8>>;
    /// Replace the profile's `<mod>.ini`.
    fn write_profile_settings(&self, mod_name: &str, text: &str) -> Result<(), String>;
}

/// A setting's type, from its key's first letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingType {
    Bool,
    Int,
    Unsigned,
    Float,
    String,
    Color,
}

impl SettingType {
    /// The type `name` (`key:Section`) declares, or `None` for a letter the game gives no type.
    pub fn of(name: &str) -> Option<Self> {
        Some(match name.chars().next()?.to_ascii_lowercase() {
            'b' => Self::Bool,
            'i' => Self::Int,
            'u' => Self::Unsigned,
            'f' => Self::Float,
            's' => Self::String,
            'r' | 'a' => Self::Color,
            _ => return None,
        })
    }
}

/// One setting's layers, as text.
#[derive(Debug, Clone, Default)]
struct Entry {
    /// `key:Section` as first spelled.
    name: String,
    default: Option<String>,
    legacy: Option<String>,
    profile: Option<String>,
}

impl Entry {
    fn value(&self) -> Option<&str> {
        self.profile.as_deref().or(self.legacy.as_deref()).or(self.default.as_deref())
    }
}

/// One mod's settings.
#[derive(Debug, Clone, Default)]
struct ModSettings {
    entries: Vec<Entry>,
    index: HashMap<String, usize>,
}

impl ModSettings {
    fn entry_mut(&mut self, name: &str) -> &mut Entry {
        let key = name.to_ascii_lowercase();
        let next = self.entries.len();
        let index = *self.index.entry(key).or_insert(next);
        if index == next {
            self.entries.push(Entry { name: name.to_owned(), ..Default::default() });
        }
        &mut self.entries[index]
    }

    fn get(&self, name: &str) -> Option<&Entry> {
        self.index.get(&name.to_ascii_lowercase()).map(|&index| &self.entries[index])
    }

    /// The profile layer as an INI file, sections and keys in the order they were first seen.
    fn profile_text(&self) -> String {
        let mut sections: Vec<(String, Vec<(String, String)>)> = Vec::new();
        for entry in &self.entries {
            let (Some(value), Some((key, section))) = (&entry.profile, entry.name.split_once(':')) else { continue };
            match sections.iter_mut().find(|(name, _)| name.eq_ignore_ascii_case(section)) {
                Some((_, keys)) => keys.push((key.to_owned(), value.clone())),
                None => sections.push((section.to_owned(), vec![(key.to_owned(), value.clone())])),
            }
        }
        let mut text = String::new();
        for (section, keys) in sections {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&format!("[{section}]\n"));
            for (key, value) in keys {
                text.push_str(&format!("{key}={value}\n"));
            }
        }
        text
    }
}

/// Parse an INI file into `(key:Section, value)` pairs, in file order. A line outside a section, a
/// comment (`;` or `#`) and a line without `=` are skipped.
pub fn parse_ini(bytes: &[u8]) -> Vec<(String, String)> {
    let text = String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)).into_owned();
    let mut section = None::<String>;
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            section = Some(name.trim().to_owned());
            continue;
        }
        let (Some(section), Some((key, value))) = (&section, line.split_once('=')) else { continue };
        let key = key.trim();
        if !key.is_empty() {
            out.push((format!("{key}:{section}"), value.trim().to_owned()));
        }
    }
    out
}

/// Every mod's settings, read the first time a mod is asked about.
#[derive(Default)]
pub struct SettingStore {
    mods: HashMap<String, ModSettings>,
    /// Mods whose profile file failed to write, for the diagnostic.
    pub write_failures: BTreeSet<String>,
}

impl SettingStore {
    fn mod_settings(&mut self, files: &dyn HelperFiles, mod_name: &str) -> &mut ModSettings {
        self.mods.entry(mod_name.to_ascii_lowercase()).or_insert_with(|| {
            let mut settings = ModSettings::default();
            for (name, value) in files.defaults(mod_name).as_deref().map(parse_ini).unwrap_or_default() {
                settings.entry_mut(&name).default = Some(value);
            }
            for (name, value) in files.legacy_settings(mod_name).as_deref().map(parse_ini).unwrap_or_default() {
                settings.entry_mut(&name).legacy = Some(value);
            }
            for (name, value) in files.profile_settings(mod_name).as_deref().map(parse_ini).unwrap_or_default() {
                settings.entry_mut(&name).profile = Some(value);
            }
            settings
        })
    }

    /// The setting's current value as text, if any file declares it.
    pub fn text(&mut self, files: &dyn HelperFiles, mod_name: &str, name: &str) -> Option<String> {
        self.mod_settings(files, mod_name).get(name).and_then(Entry::value).map(str::to_owned)
    }

    /// The setting's default (the mod's `settings.ini`), if it declares one.
    pub fn default_text(&mut self, files: &dyn HelperFiles, mod_name: &str, name: &str) -> Option<String> {
        self.mod_settings(files, mod_name).get(name).and_then(|entry| entry.default.clone())
    }

    /// `GetModSettingInt`: -1 when no file declares it.
    pub fn int(&mut self, files: &dyn HelperFiles, mod_name: &str, name: &str) -> i32 {
        self.text(files, mod_name, name).map_or(-1, |text| parse_int(&text))
    }

    /// `GetModSettingFloat`: -1 when no file declares it.
    pub fn float(&mut self, files: &dyn HelperFiles, mod_name: &str, name: &str) -> f32 {
        self.text(files, mod_name, name).map_or(-1.0, |text| text.trim().parse().unwrap_or(0.0))
    }

    /// `GetModSettingBool`: false when no file declares it. The value is read as a number, as MCM
    /// Helper's stream does, so only a non-zero number is true.
    pub fn bool(&mut self, files: &dyn HelperFiles, mod_name: &str, name: &str) -> bool {
        self.text(files, mod_name, name).is_some_and(|text| parse_int(&text) != 0)
    }

    /// Write `value` (already formatted for the setting's type) to the profile layer and commit the
    /// profile's file. A setting no file declares is not set (MCM Helper's `if (setting)`); one whose
    /// value is unchanged is not committed. Returns whether the value changed.
    pub fn set_text(&mut self, files: &dyn HelperFiles, mod_name: &str, name: &str, value: String) -> bool {
        let settings = self.mod_settings(files, mod_name);
        let key = name.to_ascii_lowercase();
        let Some(&index) = settings.index.get(&key) else {
            log::debug!("MCM Helper: {mod_name} has no setting {name} to set");
            return false;
        };
        if settings.entries[index].value() == Some(value.as_str()) {
            return false;
        }
        settings.entries[index].profile = Some(value);
        let text = settings.profile_text();
        match files.write_profile_settings(mod_name, &text) {
            Ok(()) => {
                self.write_failures.remove(&mod_name.to_ascii_lowercase());
            }
            Err(error) => {
                if self.write_failures.insert(mod_name.to_ascii_lowercase()) {
                    log::warn!("MCM Helper: the settings of {mod_name} could not be written: {error}");
                }
            }
        }
        true
    }

    /// Write a number to a setting, formatted for its type (`CommitModSetting`: a float as `%f`, a bool
    /// as 0 / 1).
    pub fn set_number(&mut self, files: &dyn HelperFiles, mod_name: &str, name: &str, value: f32) -> bool {
        let text = match SettingType::of(name) {
            Some(SettingType::Float) => format!("{value:.6}"),
            Some(SettingType::Bool) => u8::from(value != 0.0).to_string(),
            Some(SettingType::Unsigned | SettingType::Color) => (value as i64 as u32).to_string(),
            Some(SettingType::String) => value.to_string(),
            _ => (value as i32).to_string(),
        };
        self.set_text(files, mod_name, name, text)
    }

    /// Put a setting back to its `settings.ini` default (`ResetToDefault`): the profile's value goes, and
    /// so does an earlier install's, which the default must win over.
    pub fn reset(&mut self, files: &dyn HelperFiles, mod_name: &str, name: &str) -> bool {
        let Some(default) = self.default_text(files, mod_name, name) else { return false };
        self.set_text(files, mod_name, name, default)
    }
}

fn parse_int(text: &str) -> i32 {
    let text = text.trim();
    text.parse::<i64>().map(|n| n as i32).or_else(|_| text.parse::<f64>().map(|n| n as i32)).unwrap_or(0)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Files held in memory; writes are recorded.
    #[derive(Default)]
    pub(crate) struct MemoryFiles {
        pub(crate) config: HashMap<String, String>,
        pub(crate) defaults: HashMap<String, String>,
        pub(crate) legacy: HashMap<String, String>,
        pub(crate) profile: Mutex<HashMap<String, String>>,
        pub(crate) writes: Mutex<usize>,
    }

    impl HelperFiles for MemoryFiles {
        fn config(&self, mod_name: &str) -> Option<Vec<u8>> {
            self.config.get(mod_name).map(|text| text.as_bytes().to_vec())
        }
        fn defaults(&self, mod_name: &str) -> Option<Vec<u8>> {
            self.defaults.get(mod_name).map(|text| text.as_bytes().to_vec())
        }
        fn legacy_settings(&self, mod_name: &str) -> Option<Vec<u8>> {
            self.legacy.get(mod_name).map(|text| text.as_bytes().to_vec())
        }
        fn profile_settings(&self, mod_name: &str) -> Option<Vec<u8>> {
            self.profile.lock().unwrap().get(mod_name).map(|text| text.as_bytes().to_vec())
        }
        fn write_profile_settings(&self, mod_name: &str, text: &str) -> Result<(), String> {
            *self.writes.lock().unwrap() += 1;
            self.profile.lock().unwrap().insert(mod_name.to_owned(), text.to_owned());
            Ok(())
        }
    }

    fn files() -> MemoryFiles {
        let mut files = MemoryFiles::default();
        files.defaults.insert(
            "Frost".into(),
            "; defaults\n[Main]\nbEnabled=1\niLevel=3\nfRate=1.5\nsName=Ysolda\n[Other]\nuMask=7\n".into(),
        );
        files.legacy.insert("Frost".into(), "[Main]\niLevel=5\n".into());
        files
    }

    #[test]
    fn ini_files_parse_into_key_section_pairs() {
        let pairs = parse_ini(b"\xEF\xBB\xBFstray=1\n[Main]\n; note\n# note\n bEnabled = 1 \nnoequals\n[Two]\nsName=a=b\n");
        assert_eq!(pairs, [("bEnabled:Main".to_owned(), "1".to_owned()), ("sName:Two".to_owned(), "a=b".to_owned())]);
    }

    #[test]
    fn reads_layer_defaults_then_an_earlier_install_then_the_profile() {
        let files = files();
        let mut store = SettingStore::default();
        assert_eq!(store.int(&files, "Frost", "iLevel:Main"), 5, "the earlier install's value over the default");
        assert!(store.bool(&files, "Frost", "BENABLED:main"), "names compare without case");
        assert_eq!(store.float(&files, "Frost", "fRate:Main"), 1.5);
        assert_eq!(store.text(&files, "Frost", "sName:Main").as_deref(), Some("Ysolda"));
        assert_eq!(store.int(&files, "Frost", "iMissing:Main"), -1);
        assert_eq!(store.float(&files, "Frost", "fMissing:Main"), -1.0);
        assert!(!store.bool(&files, "Frost", "bMissing:Main"));
        assert_eq!(store.int(&files, "Nobody", "iLevel:Main"), -1, "a mod with no files");
    }

    #[test]
    fn a_write_goes_to_the_profile_file_only_and_only_when_it_changes() {
        let files = files();
        let mut store = SettingStore::default();
        assert!(store.set_number(&files, "Frost", "iLevel:Main", 9.0));
        assert!(!store.set_number(&files, "Frost", "iLevel:Main", 9.0), "unchanged: not committed again");
        assert!(store.set_number(&files, "Frost", "fRate:Main", 2.25));
        assert!(store.set_number(&files, "Frost", "bEnabled:Main", 0.0));
        assert!(store.set_number(&files, "Frost", "uMask:Other", 12.0));
        assert!(!store.set_number(&files, "Frost", "iUndeclared:Main", 1.0), "MCM Helper sets only declared settings");
        assert_eq!(*files.writes.lock().unwrap(), 4);
        assert_eq!(
            files.profile.lock().unwrap()["Frost"],
            "[Main]\nbEnabled=0\niLevel=9\nfRate=2.250000\n\n[Other]\nuMask=12\n"
        );
        assert!(files.legacy["Frost"].contains("iLevel=5"), "the earlier install's file is never written");
        // A new session reads the profile's file back over the other layers.
        let mut next = SettingStore::default();
        assert_eq!(next.int(&files, "Frost", "iLevel:Main"), 9);
        assert!(!next.bool(&files, "Frost", "bEnabled:Main"));
    }

    #[test]
    fn a_reset_restores_the_default_over_the_earlier_install() {
        let files = files();
        let mut store = SettingStore::default();
        assert!(store.reset(&files, "Frost", "iLevel:Main"));
        assert_eq!(store.int(&files, "Frost", "iLevel:Main"), 3);
        assert!(!store.reset(&files, "Frost", "iUndeclared:Main"));
    }

    #[test]
    fn types_follow_the_first_letter() {
        assert_eq!(SettingType::of("bX:S"), Some(SettingType::Bool));
        assert_eq!(SettingType::of("FX:S"), Some(SettingType::Float));
        assert_eq!(SettingType::of("rX:S"), Some(SettingType::Color));
        assert_eq!(SettingType::of("xX:S"), None);
    }
}
