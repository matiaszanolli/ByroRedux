//! Steam install discovery.
//!
//! Three hops, each grounded in the real files on a machine with the titles
//! installed:
//!
//! 1. **Steam root** — platform-specific well-known locations.
//! 2. **Libraries** — `<root>/steamapps/libraryfolders.vdf` lists every
//!    library folder, including ones on other drives. Steam also keeps a copy
//!    under `config/`; both are read and the union taken, because which one is
//!    current varies by client version.
//! 3. **Apps** — `<library>/steamapps/appmanifest_<appid>.acf` carries
//!    `installdir`, and the game lives at
//!    `<library>/steamapps/common/<installdir>`.
//!
//! Nothing here trusts the catalog's `install_dir` over the manifest: the
//! manifest is what Steam actually did, the catalog is only how we find the
//! manifest. A user who moved or renamed an install is still detected.

use std::path::{Path, PathBuf};

use crate::catalog::{self, SteamApp};
use crate::vdf;

/// Well-known Steam root directories for this platform, most likely first.
///
/// Only existing directories are returned, so the caller can treat the list as
/// "roots to search" without re-checking.
pub fn steam_roots() -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    #[cfg(target_os = "windows")]
    {
        // The registry (`HKCU\Software\Valve\Steam\SteamPath`) is the
        // authoritative answer and is still to do; these cover the default
        // installs, which is the large majority.
        for key in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(root) = std::env::var_os(key) {
                candidates.push(PathBuf::from(root).join("Steam"));
            }
        }
    }

    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        #[cfg(target_os = "macos")]
        candidates.push(
            home.join("Library")
                .join("Application Support")
                .join("Steam"),
        );

        candidates.push(home.join(".steam").join("steam"));
        candidates.push(home.join(".steam").join("root"));
        candidates.push(home.join(".local").join("share").join("Steam"));
        // Flatpak keeps its own prefix, and a Flatpak Steam is invisible to
        // every path above.
        candidates.push(
            home.join(".var")
                .join("app")
                .join("com.valvesoftware.Steam")
                .join("data")
                .join("Steam"),
        );
    }

    let mut roots: Vec<PathBuf> = Vec::new();
    for candidate in candidates {
        // `~/.steam/steam` and `~/.steam/root` are usually symlinks to the
        // same place; canonicalise so one install is not reported twice.
        let resolved = candidate.canonicalize().unwrap_or(candidate);
        if resolved.is_dir() && !roots.contains(&resolved) {
            roots.push(resolved);
        }
    }
    roots
}

/// Library folders declared by a Steam root, including the root itself.
pub fn library_paths(steam_root: &Path) -> Vec<PathBuf> {
    let mut libraries = vec![steam_root.to_path_buf()];
    for relative in ["steamapps/libraryfolders.vdf", "config/libraryfolders.vdf"] {
        let path = steam_root.join(relative);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let root = match vdf::parse(&text) {
            Ok(root) => root,
            Err(error) => {
                log::warn!("game-detect: could not parse {}: {error}", path.display());
                continue;
            }
        };
        let Some(folders) = root.get("libraryfolders") else {
            continue;
        };
        for (_, folder) in folders.entries() {
            let Some(path) = folder.get_str("path") else {
                continue;
            };
            let path = PathBuf::from(path);
            if path.is_dir() && !libraries.contains(&path) {
                libraries.push(path);
            }
        }
    }
    libraries
}

/// One title found in one library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SteamInstall {
    pub app: &'static SteamApp,
    /// Steam's own display name from the manifest, when it has one.
    pub name: String,
    /// `<library>/steamapps/common/<installdir>`.
    pub install_path: PathBuf,
    /// The manifest this was read from — worth surfacing when a detected
    /// install turns out to be wrong.
    pub manifest: PathBuf,
}

impl SteamInstall {
    /// The `Data` directory the engine actually loads from.
    pub fn data_dir(&self) -> PathBuf {
        self.install_path.join("Data")
    }
}

/// Titles the engine has a profile for, installed in this library.
///
/// A manifest that names an app we do not support, cannot be read, or whose
/// `installdir` no longer exists on disk is skipped — a stale manifest is
/// common (Steam leaves them behind after an uninstall) and must not surface
/// as a broken install.
pub fn installs_in_library(library: &Path) -> Vec<SteamInstall> {
    let steamapps = library.join("steamapps");
    let mut found = Vec::new();
    for app in catalog::STEAM_APPS {
        let manifest = steamapps.join(format!("appmanifest_{}.acf", app.appid));
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        let parsed = match vdf::parse(&text) {
            Ok(parsed) => parsed,
            Err(error) => {
                log::warn!(
                    "game-detect: could not parse {}: {error}",
                    manifest.display()
                );
                continue;
            }
        };
        let Some(state) = parsed.get("AppState") else {
            continue;
        };
        // Trust the manifest's `installdir` over the catalog's: it is what
        // Steam actually did on this machine.
        let install_dir = state.get_str("installdir").unwrap_or(app.install_dir);
        let install_path = steamapps.join("common").join(install_dir);
        if !install_path.is_dir() {
            log::debug!(
                "game-detect: {} manifest points at {}, which is absent (stale manifest?)",
                app.profile,
                install_path.display()
            );
            continue;
        }
        found.push(SteamInstall {
            app,
            name: state.get_str("name").unwrap_or(app.profile).to_owned(),
            install_path,
            manifest,
        });
    }
    found
}

/// Every supported title installed through Steam on this machine.
///
/// Duplicates across libraries (the same title present twice) keep the first
/// hit, which follows the root ordering in [`steam_roots`].
pub fn detect() -> Vec<SteamInstall> {
    let mut found: Vec<SteamInstall> = Vec::new();
    for root in steam_roots() {
        for library in library_paths(&root) {
            for install in installs_in_library(&library) {
                if !found.iter().any(|seen| seen.app.appid == install.app.appid) {
                    found.push(install);
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Build a throwaway Steam root: one library at the root, one on another
    /// "drive", FNV installed in the second.
    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Steam");
        let other = dir.path().join("Elsewhere");
        fs::create_dir_all(root.join("steamapps")).unwrap();
        fs::create_dir_all(other.join("steamapps").join("common")).unwrap();

        fs::write(
            root.join("steamapps/libraryfolders.vdf"),
            format!(
                "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}\n",
                root.display(),
                other.display()
            ),
        )
        .unwrap();

        fs::create_dir_all(other.join("steamapps/common/Fallout New Vegas/Data")).unwrap();
        fs::write(
            other.join("steamapps/appmanifest_22380.acf"),
            "\"AppState\"\n{\n\t\"appid\"\t\"22380\"\n\t\"name\"\t\"Fallout: New Vegas\"\n\t\"installdir\"\t\"Fallout New Vegas\"\n}\n",
        )
        .unwrap();
        (dir, root)
    }

    #[test]
    fn libraries_on_other_drives_are_followed() {
        let (dir, root) = fixture();
        let libraries = library_paths(&root);
        assert!(libraries.contains(&root));
        assert!(libraries.contains(&dir.path().join("Elsewhere")));
    }

    #[test]
    fn an_installed_title_resolves_to_its_data_dir() {
        let (dir, _root) = fixture();
        let installs = installs_in_library(&dir.path().join("Elsewhere"));
        assert_eq!(installs.len(), 1);
        assert_eq!(installs[0].app.profile, "fnv");
        assert_eq!(installs[0].name, "Fallout: New Vegas");
        assert!(installs[0].data_dir().is_dir());
    }

    /// Steam leaves manifests behind after an uninstall. A manifest whose
    /// `installdir` is gone must not surface as a detected-but-broken game.
    #[test]
    fn a_stale_manifest_without_its_install_dir_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let library = dir.path().to_path_buf();
        fs::create_dir_all(library.join("steamapps/common")).unwrap();
        fs::write(
            library.join("steamapps/appmanifest_377160.acf"),
            "\"AppState\"\n{\n\t\"installdir\"\t\"Fallout 4\"\n}\n",
        )
        .unwrap();
        assert!(installs_in_library(&library).is_empty());
    }

    /// A user who renamed or moved the folder is still detected, because the
    /// manifest — not the catalog's default — decides the path.
    #[test]
    fn the_manifest_install_dir_wins_over_the_catalog_default() {
        let dir = tempfile::tempdir().unwrap();
        let library = dir.path().to_path_buf();
        fs::create_dir_all(library.join("steamapps/common/FNV-moved/Data")).unwrap();
        fs::write(
            library.join("steamapps/appmanifest_22380.acf"),
            "\"AppState\"\n{\n\t\"installdir\"\t\"FNV-moved\"\n}\n",
        )
        .unwrap();
        let installs = installs_in_library(&library);
        assert_eq!(installs.len(), 1);
        assert!(installs[0].install_path.ends_with("FNV-moved"));
    }

    /// An unreadable library file must not take the search down with it — the
    /// root itself is still a library.
    #[test]
    fn a_corrupt_library_file_degrades_to_the_root_only() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join("steamapps")).unwrap();
        fs::write(root.join("steamapps/libraryfolders.vdf"), "\"unterminated").unwrap();
        assert_eq!(library_paths(&root), vec![root]);
    }
}
