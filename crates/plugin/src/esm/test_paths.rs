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
//!
//! #3741 (TD2-2026-08-30-01) — `pub`, not `pub(crate)`, and no longer
//! `#[cfg(test)]`: an integration test under `tests/` is a separate
//! crate that links this one as a *normal* (non-test) dependency, so it
//! structurally cannot reach a `pub(crate)` item, nor one gated on
//! `#[cfg(test)]` (that gate only applies within this crate's own
//! `cargo test` compilation unit). `crates/plugin/tests/parse_real_esm.rs`
//! (in the same package) re-hardcoded these same roots 42 times instead
//! — and diverged while doing it: it used `BYROREDUX_OBL_DATA` where
//! this module used `BYROREDUX_OBLIVION_DATA` for the identical game, a
//! real instance of exactly the "env var not consulted the same way
//! everywhere" failure #1058 set out to remove. `parse_real_esm.rs`'s own
//! `data_dir(env_var, fallback)` wrapper (existence-checked,
//! skip-on-miss — a slightly more defensive shape than the bare
//! `*_data_dir()` accessors below) is unchanged; only its call sites'
//! literal arguments now reference the
//! `*_ENV`/`*_DEFAULT` constants below instead of re-typing the
//! literals a third time.

use std::path::PathBuf;

/// Resolve a per-game data directory: env-var override falling back to
/// the canonical Steam path on the reference dev machine. The returned
/// path is NOT validated for existence — callers should check
/// `.is_dir()` / `.is_file()` and skip the test on miss.
fn data_dir(env_var: &str, default: &str) -> PathBuf {
    // #3850: an explicitly-set override is BINDING. Pre-fix this returned the
    // env var's value unchecked, so a typo'd path surfaced much later as a
    // confusing "file not found" against a path the caller never named. An
    // empty value is treated as unset, matching the usual shell convention.
    match std::env::var(env_var).ok().filter(|s| !s.is_empty()) {
        Some(v) => {
            let p = PathBuf::from(&v);
            assert!(
                p.is_dir(),
                "{env_var} points to {v:?}, which is not a directory"
            );
            p
        }
        None => PathBuf::from(default),
    }
}

pub const OBLIVION_ENV: &str = "BYROREDUX_OBLIVION_DATA";
pub const OBLIVION_DEFAULT: &str = "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data";
pub fn oblivion_data_dir() -> PathBuf {
    data_dir(OBLIVION_ENV, OBLIVION_DEFAULT)
}

pub const FNV_ENV: &str = "BYROREDUX_FNV_DATA";
pub const FNV_DEFAULT: &str = "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data";
pub fn fnv_data_dir() -> PathBuf {
    data_dir(FNV_ENV, FNV_DEFAULT)
}

pub const FO3_ENV: &str = "BYROREDUX_FO3_DATA";
pub const FO3_DEFAULT: &str = "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data";
pub fn fo3_data_dir() -> PathBuf {
    data_dir(FO3_ENV, FO3_DEFAULT)
}

pub const SKYRIM_SE_ENV: &str = "BYROREDUX_SKYRIMSE_DATA";
pub const SKYRIM_SE_DEFAULT: &str =
    "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data";
pub fn skyrim_se_data_dir() -> PathBuf {
    data_dir(SKYRIM_SE_ENV, SKYRIM_SE_DEFAULT)
}

pub const FO4_ENV: &str = "BYROREDUX_FO4_DATA";
pub const FO4_DEFAULT: &str = "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data";
pub fn fo4_data_dir() -> PathBuf {
    data_dir(FO4_ENV, FO4_DEFAULT)
}

/// #3741 — the module doc has always listed FO76 among the covered
/// games, but no accessor existed for it; `parse_real_esm.rs`'s own
/// (now-removed) hardcoded copy was the only place `BYROREDUX_FO76_DATA`
/// was consulted at all.
pub const FO76_ENV: &str = "BYROREDUX_FO76_DATA";
pub const FO76_DEFAULT: &str = "/mnt/data/SteamLibrary/steamapps/common/Fallout76/Data";
pub fn fo76_data_dir() -> PathBuf {
    data_dir(FO76_ENV, FO76_DEFAULT)
}

pub const STARFIELD_ENV: &str = "BYROREDUX_STARFIELD_DATA";
pub const STARFIELD_DEFAULT: &str = "/mnt/data/SteamLibrary/steamapps/common/Starfield/Data";
pub fn starfield_data_dir() -> PathBuf {
    data_dir(STARFIELD_ENV, STARFIELD_DEFAULT)
}

// ── ESM convenience accessors (the actual hot-path callers) ──────────

pub fn oblivion_esm() -> PathBuf {
    oblivion_data_dir().join("Oblivion.esm")
}

pub fn fnv_esm() -> PathBuf {
    fnv_data_dir().join("FalloutNV.esm")
}

pub fn fo3_esm() -> PathBuf {
    fo3_data_dir().join("Fallout3.esm")
}

pub fn skyrim_se_esm() -> PathBuf {
    skyrim_se_data_dir().join("Skyrim.esm")
}

pub fn fo4_esm() -> PathBuf {
    fo4_data_dir().join("Fallout4.esm")
}

pub fn starfield_esm() -> PathBuf {
    starfield_data_dir().join("Starfield.esm")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #3741 — every `*_ENV` constant must follow the documented
    /// `BYROREDUX_<GAME>_DATA` shape. Pins the canonical name so a future
    /// caller (in this crate or a `tests/` sibling) cannot silently
    /// reintroduce a divergent one — `BYROREDUX_OBL_DATA` for the same
    /// game `BYROREDUX_OBLIVION_DATA` names here was exactly this failure.
    #[test]
    fn every_env_name_follows_the_documented_shape() {
        for env in [
            OBLIVION_ENV,
            FNV_ENV,
            FO3_ENV,
            SKYRIM_SE_ENV,
            FO4_ENV,
            FO76_ENV,
            STARFIELD_ENV,
        ] {
            assert!(
                env.starts_with("BYROREDUX_") && env.ends_with("_DATA"),
                "{env} does not match the documented BYROREDUX_<GAME>_DATA shape"
            );
        }
    }

    /// Every `*_data_dir()` accessor must fall back to its documented
    /// default when the env var is unset — this is the behavior every
    /// caller (`parse_real_esm.rs` included) relies on for a clean skip
    /// rather than a panic on a machine with no override set.
    ///
    /// Runs with the env var explicitly removed rather than assuming it
    /// is unset in CI, since `cargo test` runs all tests in one process
    /// and a developer's shell could have one of these exported.
    #[test]
    fn data_dir_accessors_fall_back_to_their_documented_default_when_unset() {
        // SAFETY: `std::env::remove_var` is unsafe in this Rust edition
        // because env vars are process-global and concurrent access from
        // other threads is a data race. Tests run in separate threads by
        // default, but every one of the seven vars this touches is
        // exclusive to this test (no other test in this crate sets or
        // reads any of them), so there is no actual concurrent access to
        // race against.
        for (env, default, accessor) in [
            (
                OBLIVION_ENV,
                OBLIVION_DEFAULT,
                oblivion_data_dir as fn() -> PathBuf,
            ),
            (FNV_ENV, FNV_DEFAULT, fnv_data_dir as fn() -> PathBuf),
            (FO3_ENV, FO3_DEFAULT, fo3_data_dir as fn() -> PathBuf),
            (
                SKYRIM_SE_ENV,
                SKYRIM_SE_DEFAULT,
                skyrim_se_data_dir as fn() -> PathBuf,
            ),
            (FO4_ENV, FO4_DEFAULT, fo4_data_dir as fn() -> PathBuf),
            (FO76_ENV, FO76_DEFAULT, fo76_data_dir as fn() -> PathBuf),
            (
                STARFIELD_ENV,
                STARFIELD_DEFAULT,
                starfield_data_dir as fn() -> PathBuf,
            ),
        ] {
            // SAFETY: see the comment above the loop.
            unsafe {
                std::env::remove_var(env);
            }
            assert_eq!(accessor(), PathBuf::from(default));
        }
    }

    /// #3850 — an explicitly-set override is BINDING.
    ///
    /// Pre-fix `data_dir` returned the env var's value unchecked, so an
    /// operator who pointed `BYROREDUX_FNV_DATA` at a modded or
    /// DLC-stripped install — or simply typed the path wrong — got a
    /// confusing downstream failure against a directory they never named,
    /// or (in the `Option`-returning siblings) a silent fall back to the
    /// hardcoded dev-machine Steam path and results from a *different*
    /// install entirely.
    ///
    /// The strict switch's other half (`BYROREDUX_REQUIRE_GAME_DATA`) is
    /// deliberately not unit-tested: it is read process-globally by every
    /// resolver in the workspace, so setting it here would turn every
    /// concurrently-running real-data test in this binary into a panic.
    #[test]
    #[should_panic(expected = "is not a directory")]
    fn an_explicitly_set_override_that_is_not_a_directory_fails_loudly() {
        const PROBE: &str = "BYROREDUX_TEST_PATHS_BINDING_PROBE_DATA";
        // SAFETY: as above — process-global, but this probe-only name is
        // read by no other test, so no concurrent reader can observe it.
        unsafe {
            std::env::set_var(PROBE, "/nonexistent/byroredux/binding-probe");
        }
        let _ = data_dir(PROBE, "/also/nonexistent");
    }
}
