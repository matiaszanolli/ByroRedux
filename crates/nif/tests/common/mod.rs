//! Shared helpers for integration tests that consume real game content.
//!
//! Game data can't be committed to the repo, so tests driven by this module
//! resolve asset paths from environment variables and skip gracefully when
//! the data isn't available. The CI machine has no game data, so all tests
//! that depend on real NIFs are `#[ignore]` by default and opted into with
//! `cargo test -- --ignored`.
//!
//! ## Environment variables
//!
//! Each variable points at a game's `Data/` directory (or equivalent):
//!
//! | Variable                   | Game              |
//! |----------------------------|-------------------|
//! | `BYROREDUX_OBLIVION_DATA`  | Oblivion          |
//! | `BYROREDUX_FO3_DATA`       | Fallout 3         |
//! | `BYROREDUX_FNV_DATA`       | Fallout New Vegas |
//! | `BYROREDUX_SKYRIMSE_DATA`  | Skyrim SE         |
//!
//! If a variable is unset, the helper falls back to the canonical Steam
//! install path on this machine. Override the variable to point elsewhere.
//!
//! ## Example
//!
//! ```no_run
//! mod common;
//! use common::{Game, game_data_dir};
//!
//! #[test]
//! #[ignore]
//! fn walk_fnv_meshes() {
//!     let Some(data) = game_data_dir(Game::FalloutNV) else { return; };
//!     // ...
//! }
//! ```

#![allow(dead_code)] // Not every helper is used by every test file.

use byroredux_bsa::{Ba2Archive, BsaArchive};
use std::path::{Path, PathBuf};

/// Games we have real test content for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Game {
    Oblivion,
    Fallout3,
    FalloutNV,
    SkyrimSE,
    Fallout4,
    Fallout76,
    Starfield,
}

/// Which archive format the game's mesh archive uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Bsa,
    Ba2,
}

impl Game {
    pub fn env_var(self) -> &'static str {
        match self {
            Game::Oblivion => "BYROREDUX_OBLIVION_DATA",
            Game::Fallout3 => "BYROREDUX_FO3_DATA",
            Game::FalloutNV => "BYROREDUX_FNV_DATA",
            Game::SkyrimSE => "BYROREDUX_SKYRIMSE_DATA",
            Game::Fallout4 => "BYROREDUX_FO4_DATA",
            Game::Fallout76 => "BYROREDUX_FO76_DATA",
            Game::Starfield => "BYROREDUX_STARFIELD_DATA",
        }
    }

    /// Canonical Steam install path on the reference development machine.
    /// Used only as a fallback when the env var is unset — never as a hard
    /// assumption. Tests still skip when neither source resolves.
    pub fn default_path(self) -> PathBuf {
        let base = "/mnt/data/SteamLibrary/steamapps/common";
        match self {
            Game::Oblivion => PathBuf::from(format!("{base}/Oblivion/Data")),
            Game::Fallout3 => PathBuf::from(format!("{base}/Fallout 3 goty/Data")),
            Game::FalloutNV => PathBuf::from(format!("{base}/Fallout New Vegas/Data")),
            Game::SkyrimSE => PathBuf::from(format!("{base}/Skyrim Special Edition/Data")),
            Game::Fallout4 => PathBuf::from(format!("{base}/Fallout 4/Data")),
            Game::Fallout76 => PathBuf::from(format!("{base}/Fallout76/Data")),
            Game::Starfield => PathBuf::from(format!("{base}/Starfield/Data")),
        }
    }

    /// Typical filename of the primary mesh archive.
    pub fn mesh_archive(self) -> &'static str {
        match self {
            Game::Oblivion => "Oblivion - Meshes.bsa",
            Game::Fallout3 => "Fallout - Meshes.bsa",
            Game::FalloutNV => "Fallout - Meshes.bsa",
            Game::SkyrimSE => "Skyrim - Meshes0.bsa",
            Game::Fallout4 => "Fallout4 - Meshes.ba2",
            Game::Fallout76 => "SeventySix - Meshes.ba2",
            Game::Starfield => "Starfield - Meshes01.ba2",
        }
    }

    pub fn archive_kind(self) -> ArchiveKind {
        match self {
            Game::Oblivion
            | Game::Fallout3
            | Game::FalloutNV
            | Game::SkyrimSE => ArchiveKind::Bsa,
            Game::Fallout4 | Game::Fallout76 | Game::Starfield => ArchiveKind::Ba2,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Game::Oblivion => "Oblivion",
            Game::Fallout3 => "Fallout 3",
            Game::FalloutNV => "Fallout New Vegas",
            Game::SkyrimSE => "Skyrim SE",
            Game::Fallout4 => "Fallout 4",
            Game::Fallout76 => "Fallout 76",
            Game::Starfield => "Starfield",
        }
    }
}

/// Either a BSA or a BA2 archive, exposing a unified file-list / extract API
/// so test code doesn't need to branch on the format.
pub enum MeshArchive {
    Bsa(BsaArchive),
    Ba2(Ba2Archive),
}

impl MeshArchive {
    pub fn file_count(&self) -> usize {
        match self {
            MeshArchive::Bsa(a) => a.file_count(),
            MeshArchive::Ba2(a) => a.file_count(),
        }
    }

    pub fn list_files(&self) -> Vec<String> {
        match self {
            MeshArchive::Bsa(a) => a.list_files().into_iter().map(|s| s.to_string()).collect(),
            MeshArchive::Ba2(a) => a.list_files().into_iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn extract(&self, path: &str) -> std::io::Result<Vec<u8>> {
        match self {
            MeshArchive::Bsa(a) => a.extract(path),
            MeshArchive::Ba2(a) => a.extract(path),
        }
    }
}

/// Resolve the data directory for a game, falling back to the default Steam
/// path. Returns `None` and prints a skip notice if neither resolves.
pub fn game_data_dir(game: Game) -> Option<PathBuf> {
    if let Ok(val) = std::env::var(game.env_var()) {
        let path = PathBuf::from(val);
        if path.is_dir() {
            return Some(path);
        }
        eprintln!(
            "[{}] {} points to {:?} which is not a directory; falling back to default",
            game.label(),
            game.env_var(),
            path
        );
    }
    let default = game.default_path();
    if default.is_dir() {
        return Some(default);
    }
    eprintln!(
        "[{}] skipping: no data dir (set {} or install to {:?})",
        game.label(),
        game.env_var(),
        default
    );
    None
}

/// Open the primary mesh archive for a game if present. Picks BSA or BA2
/// based on the game's `archive_kind`.
pub fn open_mesh_archive(game: Game) -> Option<MeshArchive> {
    let data = game_data_dir(game)?;
    let archive_path = data.join(game.mesh_archive());
    if !archive_path.is_file() {
        eprintln!(
            "[{}] skipping: {:?} not found",
            game.label(),
            archive_path
        );
        return None;
    }
    let result = match game.archive_kind() {
        ArchiveKind::Bsa => BsaArchive::open(&archive_path).map(MeshArchive::Bsa),
        ArchiveKind::Ba2 => Ba2Archive::open(&archive_path).map(MeshArchive::Ba2),
    };
    match result {
        Ok(a) => Some(a),
        Err(e) => {
            eprintln!(
                "[{}] skipping: failed to open {:?}: {}",
                game.label(),
                archive_path,
                e
            );
            None
        }
    }
}

/// Parse outcome for a single NIF.
#[derive(Debug)]
pub struct ParseOutcome {
    pub path: String,
    pub result: Result<usize, String>,
}

impl ParseOutcome {
    pub fn is_ok(&self) -> bool {
        self.result.is_ok()
    }
}

/// Parse statistics across a batch of NIFs.
#[derive(Debug, Default)]
pub struct ParseStats {
    pub total: usize,
    pub ok: usize,
    pub failures: Vec<ParseOutcome>,
}

impl ParseStats {
    pub fn success_rate(&self) -> f64 {
        if self.total == 0 {
            return 1.0;
        }
        self.ok as f64 / self.total as f64
    }

    pub fn record(&mut self, outcome: ParseOutcome) {
        self.total += 1;
        if outcome.is_ok() {
            self.ok += 1;
        } else {
            self.failures.push(outcome);
        }
    }

    pub fn print_summary(&self, label: &str) {
        let rate = self.success_rate() * 100.0;
        eprintln!(
            "[{label}] parsed {}/{} NIFs ({:.2}% success, {} failures)",
            self.ok,
            self.total,
            rate,
            self.failures.len()
        );
        // Print up to 5 failure examples.
        for outcome in self.failures.iter().take(5) {
            if let Err(e) = &outcome.result {
                eprintln!("  FAIL {}: {}", outcome.path, e);
            }
        }
        if self.failures.len() > 5 {
            eprintln!("  ... and {} more", self.failures.len() - 5);
        }
    }
}

/// Parse every NIF inside a mesh archive (BSA or BA2) and collect stats.
/// If `limit` is `Some(n)`, only the first `n` NIFs are parsed.
pub fn parse_all_nifs_in_archive(archive: &MeshArchive, limit: Option<usize>) -> ParseStats {
    let mut stats = ParseStats::default();
    let files: Vec<String> = archive
        .list_files()
        .into_iter()
        .filter(|p| p.to_ascii_lowercase().ends_with(".nif"))
        .collect();

    let iter: Box<dyn Iterator<Item = &String>> = match limit {
        Some(n) => Box::new(files.iter().take(n)),
        None => Box::new(files.iter()),
    };

    for path in iter {
        let outcome = match archive.extract(path) {
            Err(e) => ParseOutcome {
                path: path.clone(),
                result: Err(format!("extract: {e}")),
            },
            Ok(bytes) => match byroredux_nif::parse_nif(&bytes) {
                Ok(scene) => ParseOutcome {
                    path: path.clone(),
                    result: Ok(scene.len()),
                },
                Err(e) => ParseOutcome {
                    path: path.clone(),
                    result: Err(format!("parse: {e}")),
                },
            },
        };
        stats.record(outcome);
    }

    stats
}

/// Parse every .nif under a directory (recursively) and collect stats.
pub fn parse_all_nifs_in_dir(root: &Path, limit: Option<usize>) -> ParseStats {
    let mut stats = ParseStats::default();
    let mut stack = vec![root.to_path_buf()];
    let mut count = 0usize;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let lower = path
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();
            if lower != "nif" {
                continue;
            }
            if let Some(n) = limit {
                if count >= n {
                    return stats;
                }
            }
            count += 1;
            let display = path.display().to_string();
            let outcome = match std::fs::read(&path) {
                Err(e) => ParseOutcome {
                    path: display,
                    result: Err(format!("read: {e}")),
                },
                Ok(bytes) => match byroredux_nif::parse_nif(&bytes) {
                    Ok(scene) => ParseOutcome {
                        path: display,
                        result: Ok(scene.len()),
                    },
                    Err(e) => ParseOutcome {
                        path: display,
                        result: Err(format!("parse: {e}")),
                    },
                },
            };
            stats.record(outcome);
        }
    }
    stats
}
