//! BSA archive reading and file extraction.
//!
//! ## Module layout
//!
//! Split into submodules during the Session 36 #1118 / TD9-006 refactor — the
//! 1619-line monolith now lives in:
//!
//! - [`hash`] — debug-only folder/file name hash helpers (`#[cfg(any(debug_assertions, test))]`)
//! - [`open`] — header parse + folder / file record table walk (`BsaArchive::open`)
//! - [`extract`] — per-file extraction + zlib (v103/v104) / LZ4 (v105) dispatch
//! - [`tests`] (cfg-gated) — every integration / synthetic / fixture test
//!
//! The audit's suggested v103/v104/v105 per-file split was rejected during
//! implementation: the version-specific code is small conditional branches
//! (`folder_record_size`, compression codec dispatch, `embed_file_names`
//! interpretation) inside otherwise-shared parse logic, so a version split
//! would duplicate ~80% of the body. The responsibility split here keeps the
//! shared logic shared and the version branches local to where they fire.

mod extract;
mod hash;
mod open;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::fs::File;
use std::sync::Mutex;

/// BSA format version number for Oblivion.
pub(super) const BSA_V_OBLIVION: u32 = 103;
/// BSA format version number for Fallout 3, Fallout New Vegas, and Skyrim LE.
pub(super) const BSA_V_FO3_SKYRIM: u32 = 104;
/// BSA format version number for Skyrim Special Edition.
pub(super) const BSA_V_SKYRIM_SE: u32 = 105;

/// Bethesda BSA archive reader.
///
/// Supports all three BSA versions used by the engine lineage:
/// - v103: Oblivion (16-byte folder records, zlib compression)
/// - v104: Fallout 3 / New Vegas / Skyrim LE (16-byte folder records, zlib)
/// - v105: Skyrim SE, Fallout 4 (24-byte folder records, LZ4 compression, u64 offsets)
pub struct BsaArchive {
    /// Long-lived file handle reused across `extract` calls. Pre-#360
    /// every extract reopened the archive (one `open()` syscall per
    /// extracted file — hundreds per cell load); the mutex lets us
    /// reuse a single FD even though `extract` takes `&self`.
    file: Mutex<File>,
    version: u32,
    compressed_by_default: bool,
    /// When set (flag 0x100), each file's data starts with a bstring name prefix to skip.
    embed_file_names: bool,
    /// Maps normalized file path to FileEntry.
    files: HashMap<String, FileEntry>,
}

struct FileEntry {
    /// Byte offset from start of BSA file where file data begins.
    offset: u64,
    /// Raw size field from the file record (with compression toggle bit masked off).
    size: u32,
    /// Whether compression is toggled relative to archive default. Bit 30
    /// (0x40000000) of the on-disk size word.
    compression_toggle: bool,
    /// Whether the embed-name policy is toggled relative to archive default.
    /// Bit 31 (0x80000000) of the on-disk size word. Mixed-mode BSAs (mods
    /// that flip the flag per file rather than for the whole archive) need
    /// this toggle XOR'd against the archive-level `embed_file_names`
    /// before deciding whether to skip the bstring path prefix at extract
    /// time. Vanilla Bethesda BSAs always carry a uniform per-archive
    /// policy and never set this bit. See #616 / SK-D2-03.
    embed_name_toggle: bool,
}

impl BsaArchive {
    /// BSA format version (103 = Oblivion, 104 = FO3/FNV/Skyrim LE,
    /// 105 = Skyrim SE/FO4). Mirrors `Ba2Archive::version` so tests
    /// can pin the version-dispatch path. See #587 / FO4-DIM2-05.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// List all file paths in the archive (lowercase, backslash-separated).
    pub fn list_files(&self) -> Vec<&str> {
        self.files.keys().map(|s| s.as_str()).collect()
    }

    /// Check if the archive contains a file at the given path.
    /// Path matching is case-insensitive and normalizes separators.
    pub fn contains(&self, path: &str) -> bool {
        let key = normalize_path(path);
        self.files.contains_key(&key)
    }

    /// Number of files in the archive.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}

/// Normalize a file path for lookup: lowercase, forward slashes to backslashes.
pub(super) fn normalize_path(path: &str) -> String {
    path.to_lowercase().replace('/', "\\")
}
