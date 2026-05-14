//! Real-data integration-test path helpers.
//!
//! The repo's `#[ignore]`'d integration tests need to point at on-disk
//! Bethesda data (Oblivion / FNV / FO3 / Skyrim SE / FO4 / FO76 /
//! Starfield ESMs + BSAs). Pre-#1058 each test hardcoded the audit
//! author's Steam install path; this module centralises the override
//! shape so every test resolves the same way:
//!
//! 1. If `BYROREDUX_<GAME>_DATA` env var is set, use it.
//! 2. Otherwise, fall back to the canonical Steam install path on the
//!    reference dev machine.
//! 3. Callers are responsible for skipping when the returned path does
//!    not exist (i.e. checking `.is_file()` / `.is_dir()` before reading).
//!
//! This sibling of `crates/nif/tests/common::Game` mirrors that
//! file's `default_path()` + `mesh_archive()` convention, scoped to
//! ESMs the plugin crate's tests open directly. Other crates with
//! the same need (`bsa`, `audio`, `facegen`, `spt`, the `byroredux`
//! binary's tests) keep their per-file helpers — promoting to a
//! workspace-level utility crate is out of scope for the issue that
//! introduced this module (#1058).

use std::path::PathBuf;

/// Resolve a per-game data directory: env-var override falling back to
/// the canonical Steam path on the reference dev machine. The returned
/// path is NOT validated for existence — callers should check
/// `.is_dir()` / `.is_file()` and skip the test on miss.
fn data_dir(env_var: &str, default: &str) -> PathBuf {
    std::env::var(env_var)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(default))
}

pub(crate) fn oblivion_data_dir() -> PathBuf {
    data_dir(
        "BYROREDUX_OBLIVION_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data",
    )
}

pub(crate) fn fnv_data_dir() -> PathBuf {
    data_dir(
        "BYROREDUX_FNV_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data",
    )
}

pub(crate) fn fo3_data_dir() -> PathBuf {
    data_dir(
        "BYROREDUX_FO3_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data",
    )
}

pub(crate) fn skyrim_se_data_dir() -> PathBuf {
    data_dir(
        "BYROREDUX_SKYRIMSE_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data",
    )
}

pub(crate) fn fo4_data_dir() -> PathBuf {
    data_dir(
        "BYROREDUX_FO4_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data",
    )
}

// ── ESM convenience accessors (the actual hot-path callers) ──────────

pub(crate) fn oblivion_esm() -> PathBuf {
    oblivion_data_dir().join("Oblivion.esm")
}

pub(crate) fn fnv_esm() -> PathBuf {
    fnv_data_dir().join("FalloutNV.esm")
}

pub(crate) fn fo3_esm() -> PathBuf {
    fo3_data_dir().join("Fallout3.esm")
}

pub(crate) fn skyrim_se_esm() -> PathBuf {
    skyrim_se_data_dir().join("Skyrim.esm")
}

pub(crate) fn fo4_esm() -> PathBuf {
    fo4_data_dir().join("Fallout4.esm")
}
