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
    /// The file is re-parsed as a generic TOML document and only the `[roots]`
    /// table is replaced, so a hand-curated `[profiles.*]` block or
    /// `[defaults]` table survives verbatim — including comments' absence
    /// being the only casualty, which is why detection writes here rather than
    /// into the block a user is likely to have edited.
    pub fn merge_into_file(&self, path: impl AsRef<Path>) -> Result<(), OverrideError> {
        let path = path.as_ref();
        let mut document: toml::Table = match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map_err(|source| OverrideError::Parse {
                path: path.to_path_buf(),
                source,
            })?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => toml::Table::new(),
            Err(source) => {
                return Err(OverrideError::Write {
                    path: path.to_path_buf(),
                    source,
                })
            }
        };

        // Start from what is already there so a detection run that finds four
        // of five games does not drop the fifth's remembered path.
        let mut merged = match document.get(ROOTS_TABLE) {
            Some(toml::Value::Table(existing)) => existing.clone(),
            _ => toml::Table::new(),
        };
        for (profile, root) in &self.roots {
            merged.insert(profile.clone(), toml::Value::String(root.clone()));
        }
        document.insert(ROOTS_TABLE.to_owned(), toml::Value::Table(merged));

        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|source| OverrideError::Write {
                path: path.to_path_buf(),
                source,
            })?;
        }
        let text = toml::to_string_pretty(&document).map_err(|error| OverrideError::Write {
            path: path.to_path_buf(),
            source: std::io::Error::other(error),
        })?;
        std::fs::write(path, text).map_err(|source| OverrideError::Write {
            path: path.to_path_buf(),
            source,
        })
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
