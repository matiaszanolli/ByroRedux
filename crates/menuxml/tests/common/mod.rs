//! Real-data resolution shared by the vanilla menu-corpus tests.
//!
//! #4660 — each corpus file used to resolve its own install: an override was
//! read but never checked, and every archive went through
//! `BsaArchive::open(..).ok()?`, so a broken archive reader read as "data not
//! found; skipping". These follow the #3850 contract every other real-data
//! harness uses: a set override is binding, an absent install or archive fails
//! under `BYROREDUX_REQUIRE_GAME_DATA`, and a present archive that fails to
//! open is always a failure.

use byroredux_bsa::BsaArchive;
use std::path::{Path, PathBuf};

fn strict() -> bool {
    std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok_and(|v| v != "0")
}

/// `env_var` if set (and it must name a directory), else `default` when it
/// exists; `None` means skip, and is never returned under the strict lane.
#[track_caller]
pub fn data_dir(env_var: &str, default: &str) -> Option<PathBuf> {
    if let Some(v) = std::env::var(env_var).ok().filter(|s| !s.is_empty()) {
        let p = PathBuf::from(&v);
        assert!(p.is_dir(), "{env_var} points to {v:?}, which is not a directory");
        return Some(p);
    }
    let p = PathBuf::from(default);
    if p.is_dir() {
        return Some(p);
    }
    if strict() {
        panic!(
            "BYROREDUX_REQUIRE_GAME_DATA is set, but no game data was found: \
             {env_var} is unset and the default {p:?} is not a directory"
        );
    }
    eprintln!("skipping: {env_var} unset and {p:?} missing");
    None
}

/// Open `dir/name`; `None` (skip) only when the file is absent outside the
/// strict lane.
#[track_caller]
pub fn open_archive(dir: &Path, name: &str) -> Option<BsaArchive> {
    let path = dir.join(name);
    if !path.is_file() {
        if strict() {
            panic!("BYROREDUX_REQUIRE_GAME_DATA is set, but {path:?} is not a file");
        }
        eprintln!("skipping: {path:?} not found");
        return None;
    }
    Some(BsaArchive::open(&path).unwrap_or_else(|e| panic!("failed to open {path:?}: {e}")))
}
