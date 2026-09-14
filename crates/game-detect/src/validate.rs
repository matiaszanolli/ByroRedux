//! Pre-launch validation — the "is this install actually going to work" pass.
//!
//! Detection alone is not user-friendly. Today every one of these failures
//! reaches the user as a line on stderr, or as silence followed by a visually
//! broken scene; converting them into a report the launcher can render before
//! enabling Play is the single clearest improvement in the plan.
//!
//! Two rules keep the report honest:
//!
//! 1. **Use the real readers.** Archive presence is one thing, but "can
//!    `byroredux-bsa` actually open this" is the question that matters, so the
//!    check calls the same code the engine will. A validator that disagrees
//!    with the loader is worse than no validator.
//! 2. **Apply the sibling rule.** A `…0`-suffixed archive drags in its whole
//!    numbered series, so an absent `Textures1.bsa` beside a present
//!    `Textures0.bsa` is *not* a finding — it is a sibling that simply does
//!    not exist in this edition. The rule is imported from `byroredux-bsa`
//!    rather than restated here.

use std::path::{Path, PathBuf};

use byroredux_bsa::{numeric_sibling_paths, Ba2Archive, BsaArchive};
use byroredux_core::ecs::GameProfileEntry;

/// How bad one finding is.
///
/// `Fail` gates Play; `Warn` does not. The split follows what the user loses:
/// meshes and textures are the game, so their absence is a `Fail`, while
/// scripts, sounds, and materials degrade a launch that still works.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Ok,
    Warn,
    Fail,
}

/// One line of the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub severity: Severity,
    /// Short subject, e.g. `"Textures"` or `"Main plugin"`.
    pub label: String,
    /// One plain sentence the launcher can render as-is.
    pub detail: String,
}

impl Check {
    fn new(severity: Severity, label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            severity,
            label: label.into(),
            detail: detail.into(),
        }
    }
}

/// The verdict on one install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReport {
    pub profile: String,
    pub data_dir: PathBuf,
    pub checks: Vec<Check>,
}

impl ValidationReport {
    /// Worst severity in the report.
    pub fn verdict(&self) -> Severity {
        self.checks
            .iter()
            .map(|check| check.severity)
            .max()
            .unwrap_or(Severity::Ok)
    }

    /// Whether the launcher should enable Play.
    pub fn is_launchable(&self) -> bool {
        self.verdict() < Severity::Fail
    }

    /// Findings at or above `severity`, for a summary line.
    pub fn at_least(&self, severity: Severity) -> impl Iterator<Item = &Check> {
        self.checks
            .iter()
            .filter(move |check| check.severity >= severity)
    }
}

/// Archive categories, paired with how much their absence costs.
fn categories(entry: &GameProfileEntry) -> [(&'static str, &Vec<String>, Severity); 5] {
    [
        ("Meshes", &entry.default_bsas, Severity::Fail),
        ("Textures", &entry.default_textures_bsas, Severity::Fail),
        ("Scripts", &entry.default_scripts_bsas, Severity::Warn),
        ("Sounds", &entry.default_sounds_bsas, Severity::Warn),
        ("Materials", &entry.default_materials_bsas, Severity::Warn),
    ]
}

/// Can `byroredux-bsa` open this file? Header/table walk only — no extraction.
fn archive_opens(path: &Path) -> Result<(), String> {
    let extension = path
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let result = match extension.as_str() {
        "ba2" => Ba2Archive::open(path)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        _ => BsaArchive::open(path)
            .map(|_| ())
            .map_err(|e| e.to_string()),
    };
    result
}

/// Validate one profile against a resolved data directory.
///
/// `data_dir` is passed explicitly rather than read from `entry.root`, because
/// the launcher validates a *candidate* it just detected — before deciding
/// whether to write it back as an override.
pub fn validate(entry: &GameProfileEntry, data_dir: &Path) -> ValidationReport {
    // Validate the release actually installed here, the same one `--game`
    // expansion will launch — otherwise a complete 2011 Skyrim would be
    // failed for lacking Special Edition's archive names.
    let entry = &entry.for_data_dir(data_dir);
    let mut checks = Vec::new();

    if !data_dir.is_dir() {
        checks.push(Check::new(
            Severity::Fail,
            "Data folder",
            format!("{} does not exist or is not readable", data_dir.display()),
        ));
        // Every later check is derived from this path; reporting five more
        // failures that all say the same thing would bury the real one.
        return ValidationReport {
            profile: entry.name.clone(),
            data_dir: data_dir.to_path_buf(),
            checks,
        };
    }

    let esm = data_dir.join(&entry.esm);
    if entry.esm.trim().is_empty() {
        checks.push(Check::new(
            Severity::Fail,
            "Main plugin",
            "the profile does not name an ESM",
        ));
    } else if !esm.is_file() {
        checks.push(Check::new(
            Severity::Fail,
            "Main plugin",
            format!("{} is missing", entry.esm),
        ));
    } else {
        let size_mb = esm.metadata().map(|m| m.len()).unwrap_or(0) / (1024 * 1024);
        checks.push(Check::new(
            Severity::Ok,
            "Main plugin",
            format!("{} ({size_mb} MB)", entry.esm),
        ));
    }

    for (label, names, missing_severity) in categories(entry) {
        if names.is_empty() {
            // An empty *optional* list is a design fact, not a gap:
            // `default_materials_bsas` is empty for every game before FO4.
            // An empty mesh or texture list is different — it means the
            // profile itself is unfinished, and the engine will load the plugin
            // and then find no geometry. Caught on real data: the shipped
            // Starfield profile lists no archives at all, and the report said
            // "ready".
            if missing_severity == Severity::Fail {
                checks.push(Check::new(
                    Severity::Warn,
                    label,
                    "the profile lists no archives; content may not load".to_owned(),
                ));
            }
            continue;
        }
        let mut loaded = 0usize;
        for name in names {
            let path = data_dir.join(name);
            if !path.is_file() {
                checks.push(Check::new(
                    missing_severity,
                    label,
                    format!("{name} is missing"),
                ));
                continue;
            }
            if let Err(error) = archive_opens(&path) {
                // Present but unreadable is worse than absent: it usually means
                // a truncated download or a mod manager that mangled the file,
                // and the engine would fail mid-load rather than at startup.
                checks.push(Check::new(
                    Severity::Fail,
                    label,
                    format!("{name} could not be opened ({error})"),
                ));
                continue;
            }
            loaded += 1;
            // Siblings are auto-loaded by the engine, so count them into the
            // total — but never report an absent one, which is the whole point
            // of consulting the rule here.
            loaded += numeric_sibling_paths(&path.to_string_lossy())
                .into_iter()
                .filter(|sibling| Path::new(sibling).is_file())
                .count();
        }
        if loaded > 0 {
            checks.push(Check::new(
                Severity::Ok,
                label,
                format!("{loaded} archive(s) will load"),
            ));
        }
    }

    ValidationReport {
        profile: entry.name.clone(),
        data_dir: data_dir.to_path_buf(),
        checks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn entry() -> GameProfileEntry {
        GameProfileEntry {
            name: "Test Game".into(),
            esm: "Test.esm".into(),
            default_bsas: vec!["Test - Meshes.bsa".into()],
            default_textures_bsas: vec!["Test - Textures0.bsa".into()],
            ..GameProfileEntry::default()
        }
    }

    /// Real BSA v104 header: magic, version, folder-record offset, flags, then
    /// zero counts — enough for `BsaArchive::open` to walk an empty table.
    fn write_empty_bsa(path: &Path) {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"BSA\0");
        bytes.extend_from_slice(&104u32.to_le_bytes()); // version
        bytes.extend_from_slice(&36u32.to_le_bytes()); // folder records offset
        bytes.extend_from_slice(&3u32.to_le_bytes()); // archive flags
        for _ in 0..6 {
            bytes.extend_from_slice(&0u32.to_le_bytes()); // counts + lengths
        }
        bytes.extend_from_slice(&0u32.to_le_bytes()); // file flags
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn a_missing_data_dir_reports_once_and_stops() {
        let report = validate(&entry(), Path::new("/nonexistent/Data"));
        assert_eq!(report.checks.len(), 1);
        assert_eq!(report.verdict(), Severity::Fail);
        assert!(!report.is_launchable());
    }

    #[test]
    fn a_complete_install_is_launchable() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Test.esm"), b"TES4").unwrap();
        write_empty_bsa(&dir.path().join("Test - Meshes.bsa"));
        write_empty_bsa(&dir.path().join("Test - Textures0.bsa"));

        let report = validate(&entry(), dir.path());
        assert_eq!(report.verdict(), Severity::Ok, "{:#?}", report.checks);
        assert!(report.is_launchable());
    }

    /// The sibling rule, in the direction that matters: present siblings are
    /// counted, and the absent ones in the series are never reported.
    #[test]
    fn present_siblings_are_counted_and_absent_ones_are_not_reported() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Test.esm"), b"TES4").unwrap();
        write_empty_bsa(&dir.path().join("Test - Meshes.bsa"));
        write_empty_bsa(&dir.path().join("Test - Textures0.bsa"));
        // Only two of the nine possible siblings exist, which is normal.
        write_empty_bsa(&dir.path().join("Test - Textures1.bsa"));
        write_empty_bsa(&dir.path().join("Test - Textures2.bsa"));

        let report = validate(&entry(), dir.path());
        let textures = report
            .checks
            .iter()
            .find(|check| check.label == "Textures")
            .unwrap();
        assert_eq!(textures.detail, "3 archive(s) will load");
        assert!(report.is_launchable());
        assert!(
            report.at_least(Severity::Warn).next().is_none(),
            "absent siblings must not be findings: {:#?}",
            report.checks
        );
    }

    #[test]
    fn a_missing_mesh_archive_fails_but_a_missing_sound_archive_only_warns() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Test.esm"), b"TES4").unwrap();
        write_empty_bsa(&dir.path().join("Test - Textures0.bsa"));

        let mut with_sounds = entry();
        with_sounds.default_sounds_bsas = vec!["Test - Sounds.bsa".into()];
        let report = validate(&with_sounds, dir.path());

        let by_label = |label: &str| {
            report
                .checks
                .iter()
                .find(|check| check.label == label && check.severity != Severity::Ok)
                .map(|check| check.severity)
        };
        assert_eq!(by_label("Meshes"), Some(Severity::Fail));
        assert_eq!(by_label("Sounds"), Some(Severity::Warn));
        assert!(!report.is_launchable());
    }

    /// Present-but-unreadable is a `Fail` even in a `Warn` category: the engine
    /// would fail mid-load rather than start degraded.
    #[test]
    fn a_corrupt_archive_fails_regardless_of_its_category() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Test.esm"), b"TES4").unwrap();
        write_empty_bsa(&dir.path().join("Test - Meshes.bsa"));
        write_empty_bsa(&dir.path().join("Test - Textures0.bsa"));
        fs::write(dir.path().join("Test - Sounds.bsa"), b"not an archive").unwrap();

        let mut with_sounds = entry();
        with_sounds.default_sounds_bsas = vec!["Test - Sounds.bsa".into()];
        let report = validate(&with_sounds, dir.path());
        let sounds = report
            .checks
            .iter()
            .find(|check| check.label == "Sounds")
            .unwrap();
        assert_eq!(sounds.severity, Severity::Fail);
        assert!(sounds.detail.contains("could not be opened"));
    }

    /// An empty category is a design fact (no game before FO4 has materials
    /// archives), not a gap, and must produce no line at all.
    /// Caught by running against a real install: the shipped Starfield profile
    /// lists no archives at all, and an "empty list is fine" rule reported it
    /// as `ready`. Empty is fine for materials, never for meshes or textures.
    #[test]
    fn a_profile_with_no_mesh_archives_is_not_silently_ready() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Test.esm"), b"TES4").unwrap();

        let bare = GameProfileEntry {
            name: "Bare".into(),
            esm: "Test.esm".into(),
            ..GameProfileEntry::default()
        };
        let report = validate(&bare, dir.path());
        assert_eq!(report.verdict(), Severity::Warn, "{:#?}", report.checks);
        assert!(report.is_launchable(), "a warning must not gate Play");

        let labels: Vec<&str> = report
            .at_least(Severity::Warn)
            .map(|check| check.label.as_str())
            .collect();
        assert_eq!(labels, ["Meshes", "Textures"]);
        // Materials being empty is normal everywhere before FO4.
        assert!(report.checks.iter().all(|check| check.label != "Materials"));
    }

    /// A profile with an alternate release whose archive names differ, the
    /// shape of 2011 Skyrim (`Meshes.bsa`) beside Special Edition
    /// (`Meshes0.bsa`).
    fn entry_with_release() -> GameProfileEntry {
        GameProfileEntry {
            releases: vec![byroredux_core::ecs::GameRelease {
                name: "Test Game (Original)".into(),
                default_bsas: Some(vec!["Old - Meshes.bsa".into()]),
                default_textures_bsas: Some(vec!["Old - Textures.bsa".into()]),
                ..Default::default()
            }],
            ..entry()
        }
    }

    /// The files choose the release: an install carrying only the alternate
    /// release's archives is complete, not a failed primary release.
    #[test]
    fn an_alternate_release_is_validated_against_its_own_archives() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Test.esm"), b"TES4").unwrap();
        write_empty_bsa(&dir.path().join("Old - Meshes.bsa"));
        write_empty_bsa(&dir.path().join("Old - Textures.bsa"));

        let report = validate(&entry_with_release(), dir.path());
        assert_eq!(report.verdict(), Severity::Ok, "{:#?}", report.checks);
        assert_eq!(report.profile, "Test Game (Original)");
    }

    /// Both releases' archives present (a hybrid folder): the primary wins, so
    /// adding a release never changes what an existing install launches.
    #[test]
    fn the_primary_release_wins_when_its_archives_are_all_present() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "Test - Meshes.bsa",
            "Test - Textures0.bsa",
            "Old - Meshes.bsa",
            "Old - Textures.bsa",
        ] {
            write_empty_bsa(&dir.path().join(name));
        }
        let chosen = entry_with_release().for_data_dir(dir.path());
        assert_eq!(chosen.name, "Test Game");
        assert_eq!(chosen.default_bsas, ["Test - Meshes.bsa"]);
        assert!(chosen.releases.is_empty());
    }

    /// No release complete: report the primary release's missing archives, not
    /// a mix, and not whichever alternate happened to be listed last.
    #[test]
    fn an_incomplete_install_is_reported_against_the_primary_release() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Test.esm"), b"TES4").unwrap();
        write_empty_bsa(&dir.path().join("Old - Meshes.bsa"));

        let report = validate(&entry_with_release(), dir.path());
        assert!(!report.is_launchable());
        assert!(
            report
                .checks
                .iter()
                .any(|check| check.detail == "Test - Meshes.bsa is missing"),
            "{:#?}",
            report.checks
        );
    }

    #[test]
    fn an_empty_archive_category_produces_no_check() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Test.esm"), b"TES4").unwrap();
        write_empty_bsa(&dir.path().join("Test - Meshes.bsa"));
        write_empty_bsa(&dir.path().join("Test - Textures0.bsa"));

        let report = validate(&entry(), dir.path());
        assert!(report.checks.iter().all(|check| check.label != "Materials"));
    }
}
