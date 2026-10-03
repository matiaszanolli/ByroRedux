//! Detected install paths, written back so the *engine* benefits too.
//!
//! Detection only ever learns **where** a game is, never which archives it
//! needs. That distinction drives the file format.
//!
//! The obvious write-back — emit a full `[profiles.<key>]` block into
//! `~/.byroredux/profiles.toml` — is wrong, because the profile loader merges
//! by whole-entry replacement: a user block *shadows* the shipped one. A
//! write-back carrying a copy of today's archive lists would silently freeze
//! them, so a later engine update that adds an archive to a shipped profile
//! would have no effect on any machine detection had ever touched.
//!
//! So detection writes a narrower thing: a `[roots]` table of
//! `<profile key> = "<absolute data dir>"`, applied *over* the merged registry
//! and touching only `root`. It cannot clobber curated profile data, which is
//! also what makes it safe to write without asking.
//!
//! ```toml
//! # ~/.byroredux/profiles.toml
//! [roots]
//! fnv = "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data"
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Table name inside the per-user profiles file.
pub const ROOTS_TABLE: &str = "roots";

/// `profile key → absolute data directory`, as stored under `[roots]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootOverrides {
    #[serde(default)]
    pub roots: BTreeMap<String, String>,
}

#[derive(Debug, thiserror::Error)]
pub enum OverrideError {
    #[error("could not read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not valid TOML: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

impl RootOverrides {
    /// Read the `[roots]` table out of a profiles file.
    ///
    /// A missing file is an empty set, not an error — that is the ordinary
    /// state before the user has ever run detection. Every other key in the
    /// file (`[profiles.*]`, `[defaults]`) is ignored, so this can be pointed
    /// at the same file the profile loader reads.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, OverrideError> {
        let path = path.as_ref();
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default())
            }
            Err(source) => {
                return Err(OverrideError::Read {
                    path: path.to_path_buf(),
                    source,
                })
            }
        };
        toml::from_str(&text).map_err(|source| OverrideError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Merge these overrides into a profiles file, preserving everything else
    /// in it.
    ///
    /// Only the `[roots]` entries this set carries are edited, through a
    /// format-preserving TOML document: comments, key order and formatting in
    /// a hand-curated `[profiles.*]` block, `[defaults]` table — and the rest
    /// of `[roots]` — survive byte for byte. #5166 — this used to round-trip
    /// the whole file through `toml::Table`, which kept the values but dropped
    /// every comment, and the launcher does it before every Play.
    ///
    /// When every root already holds the value it would be given, the file is
    /// not written at all, so an unchanged detection is a read, not a rewrite.
    pub fn merge_into_file(&self, path: impl AsRef<Path>) -> Result<(), OverrideError> {
        let path = path.as_ref();
        let (original, exists) = match std::fs::read_to_string(path) {
            Ok(text) => (text, true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (String::new(), false),
            Err(source) => {
                return Err(OverrideError::Read {
                    path: path.to_path_buf(),
                    source,
                })
            }
        };
        // Validate with the same parser `load` and the profile loader use, so
        // a malformed file is refused with the same error type and never
        // rewritten.
        toml::from_str::<toml::Table>(&original).map_err(|source| OverrideError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        let mut document: toml_edit::DocumentMut =
            original.parse().map_err(|error| OverrideError::Write {
                path: path.to_path_buf(),
                source: std::io::Error::other(error),
            })?;

        // Start from what is already there so a detection run that finds four
        // of five games does not drop the fifth's remembered path.
        let roots = document.entry(ROOTS_TABLE).or_insert_with(toml_edit::table);
        if !roots.is_table_like() {
            *roots = toml_edit::table();
        }
        let roots = roots
            .as_table_like_mut()
            .expect("`[roots]` was just made a table");
        let mut changed = false;
        for (profile, root) in &self.roots {
            if roots.get(profile).and_then(toml_edit::Item::as_str) != Some(root.as_str()) {
                roots.insert(profile, toml_edit::value(root.as_str()));
                changed = true;
            }
        }
        if !changed && exists {
            return Ok(());
        }

        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|source| OverrideError::Write {
                path: path.to_path_buf(),
                source,
            })?;
        }
        let text = document.to_string();
        let temp_path = byroredux_core::atomic_file::atomic_temp_path(path);
        // #5143 — no non-atomic fallback on `atomic_write` failure; see
        // `BootRequest::save` for why the old "Windows rename" branch
        // guarded nothing real and could clobber the good file.
        byroredux_core::atomic_file::atomic_write(path, &temp_path, text.as_bytes()).map_err(
            |source| OverrideError::Write {
                path: path.to_path_buf(),
                source,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overrides(pairs: &[(&str, &str)]) -> RootOverrides {
        RootOverrides {
            roots: pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        }
    }

    #[test]
    fn merge_uses_the_shared_atomic_file_writer() {
        // #5143 — also pins that the non-atomic "Windows rename" fallback
        // (which ran on any `atomic_write` error and could copy the temp's
        // partial bytes over the good file) stays gone; the failure it
        // mishandled is OS-level and cannot be fault-injected portably.
        // Scans the production text only (#5164).
        byroredux_core::atomic_file::assert_no_clobber_fallback(include_str!("overrides.rs"));
    }

    #[test]
    fn a_missing_file_reads_as_an_empty_set() {
        assert_eq!(
            RootOverrides::load("/nonexistent/profiles.toml").unwrap(),
            RootOverrides::default()
        );
    }

    #[test]
    fn write_then_read_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/profiles.toml");
        let written = overrides(&[("fnv", "/games/FNV/Data"), ("fo4", "/games/FO4/Data")]);
        written.merge_into_file(&path).unwrap();
        assert_eq!(RootOverrides::load(&path).unwrap(), written);
    }

    /// The whole point of the `[roots]` design: a user's curated profile block
    /// must survive a detection run untouched.
    #[test]
    fn merging_preserves_unrelated_tables() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.toml");
        std::fs::write(
            &path,
            "[defaults]\ngame = \"fnv\"\n\n[profiles.custom]\nname = \"Modded FNV\"\nesm = \"Custom.esm\"\ndefault_bsas = [\"A.bsa\"]\n",
        )
        .unwrap();

        overrides(&[("fnv", "/games/FNV/Data")])
            .merge_into_file(&path)
            .unwrap();

        let document: toml::Table =
            toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(document["defaults"]["game"].as_str(), Some("fnv"));
        assert_eq!(
            document["profiles"]["custom"]["name"].as_str(),
            Some("Modded FNV")
        );
        assert_eq!(
            document["profiles"]["custom"]["default_bsas"][0].as_str(),
            Some("A.bsa")
        );
        assert_eq!(document["roots"]["fnv"].as_str(), Some("/games/FNV/Data"));
    }

    /// #5166 — comments, key order and formatting outside the edited roots
    /// survive byte for byte; only the changed entry moves.
    #[test]
    fn merging_preserves_comments_and_formatting() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.toml");
        let original = "# my install notes\n[defaults]\ngame = \"fnv\"   # the usual\n\n\
                        [profiles.custom]\n# modded\nname = \"Modded FNV\"\nesm = \"Custom.esm\"\n\n\
                        [roots]\n# external drive\nfo4 = \"/games/FO4/Data\"\nfnv = \"/old/FNV/Data\"\n";
        std::fs::write(&path, original).unwrap();

        overrides(&[("fnv", "/games/FNV/Data")])
            .merge_into_file(&path)
            .unwrap();

        let written = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            written,
            original.replace("/old/FNV/Data", "/games/FNV/Data"),
            "only the changed root may differ"
        );
    }

    /// #5166 — the launcher merges before every Play; an unchanged detection
    /// must not rewrite the file at all. A write always renames a fresh temp
    /// into place, so an unchanged inode means nothing was written.
    #[cfg(unix)]
    #[test]
    fn merging_unchanged_roots_does_not_write() {
        use std::os::unix::fs::MetadataExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.toml");
        let found = overrides(&[("fnv", "/games/FNV/Data")]);
        found.merge_into_file(&path).unwrap();
        let before = std::fs::metadata(&path).unwrap().ino();

        found.merge_into_file(&path).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().ino(),
            before,
            "an unchanged merge is a read, not a rewrite"
        );

        overrides(&[("fnv", "/moved/FNV/Data")])
            .merge_into_file(&path)
            .unwrap();
        assert_ne!(
            std::fs::metadata(&path).unwrap().ino(),
            before,
            "a changed root is written"
        );
    }

    /// A malformed file is refused, not rewritten.
    #[test]
    fn merging_into_malformed_toml_is_refused_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.toml");
        std::fs::write(&path, "not = = toml").unwrap();
        let result = overrides(&[("fnv", "/games/FNV/Data")]).merge_into_file(&path);
        assert!(matches!(result, Err(OverrideError::Parse { .. })));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "not = = toml");
    }

    /// A later run that finds fewer games must not forget the ones it found
    /// before — a user unplugging an external drive should not lose the path
    /// to a game on their internal one.
    #[test]
    fn merging_keeps_previously_remembered_roots() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.toml");
        overrides(&[("fnv", "/games/FNV/Data"), ("fo4", "/games/FO4/Data")])
            .merge_into_file(&path)
            .unwrap();
        overrides(&[("fnv", "/moved/FNV/Data")])
            .merge_into_file(&path)
            .unwrap();

        let read = RootOverrides::load(&path).unwrap();
        assert_eq!(read.roots["fnv"], "/moved/FNV/Data", "re-detection updates");
        assert_eq!(
            read.roots["fo4"], "/games/FO4/Data",
            "absent stays remembered"
        );
    }

    /// The file is shared with the profile loader, so unrelated content must
    /// not make the roots unreadable.
    #[test]
    fn a_profiles_only_file_reads_as_no_overrides() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.toml");
        std::fs::write(
            &path,
            "[profiles.fnv]\nname = \"FNV\"\nesm = \"FalloutNV.esm\"\n",
        )
        .unwrap();
        assert!(RootOverrides::load(&path).unwrap().roots.is_empty());
    }
}
