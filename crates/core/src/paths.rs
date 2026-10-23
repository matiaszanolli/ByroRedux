//! Where files the engine ships, and files it writes, resolve relative to the
//! running executable (#6274).
//!
//! The rule is "next to the executable, so a launcher, a read-only working
//! directory or a packaged install works" — and it used to be re-derived per
//! subsystem (presets, profiles, the Vulkan pipeline cache, generated height
//! maps, the launcher) with different fallback orders. It is stated once here:
//!
//! * **Shipped read-only assets** ([`shipped_asset`]): the working directory
//!   first (`cargo run` from the repo root), then the executable's directory
//!   (a release build beside its `assets/`).
//! * **Writable caches** ([`cache_dir`] / [`cache_file`]): `BYRO_CACHE_DIR`
//!   when set (tests, read-only installs), else the executable's directory,
//!   else the working directory. A cache never prefers the working directory,
//!   so it survives a launcher changing it.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Environment override for the parent of every cache this module resolves.
pub const CACHE_DIR_ENV: &str = "BYRO_CACHE_DIR";

/// The directory holding the running executable, when it can be determined.
pub fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
}

/// Where `rel` could be found, in lookup order: relative to the working
/// directory, then to the executable's directory. Not filtered by existence.
pub fn shipped_asset_candidates(rel: impl AsRef<Path>) -> Vec<PathBuf> {
    shipped_asset_candidates_in(rel.as_ref(), exe_dir())
}

/// The first of [`shipped_asset_candidates`] that exists.
pub fn shipped_asset(rel: impl AsRef<Path>) -> Option<PathBuf> {
    shipped_asset_candidates(rel).into_iter().find(|path| path.exists())
}

/// The directory caches live under: [`CACHE_DIR_ENV`], else the executable's
/// directory, else `.`.
pub fn cache_root() -> PathBuf {
    cache_root_from(std::env::var_os(CACHE_DIR_ENV), exe_dir())
}

/// A cache subdirectory, `<cache root>/<name>`. Not created.
pub fn cache_dir(name: impl AsRef<Path>) -> PathBuf {
    cache_root().join(name)
}

/// A cache file, `<cache root>/<name>`.
pub fn cache_file(name: impl AsRef<Path>) -> PathBuf {
    cache_root().join(name)
}

fn shipped_asset_candidates_in(rel: &Path, exe_dir: Option<PathBuf>) -> Vec<PathBuf> {
    let mut out = vec![rel.to_path_buf()];
    if let Some(dir) = exe_dir {
        out.push(dir.join(rel));
    }
    out
}

fn cache_root_from(env: Option<OsString>, exe_dir: Option<PathBuf>) -> PathBuf {
    env.filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or(exe_dir)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_assets_try_the_working_directory_before_the_executable() {
        let candidates = shipped_asset_candidates_in(Path::new("assets/x.toml"), Some("/opt/byro".into()));
        assert_eq!(candidates, [PathBuf::from("assets/x.toml"), PathBuf::from("/opt/byro/assets/x.toml")]);
        assert_eq!(shipped_asset_candidates_in(Path::new("a"), None), [PathBuf::from("a")]);
    }

    #[test]
    fn cache_root_prefers_the_override_then_the_executable_then_cwd() {
        let exe = || Some(PathBuf::from("/opt/byro"));
        assert_eq!(cache_root_from(Some("/tmp/c".into()), exe()), PathBuf::from("/tmp/c"));
        assert_eq!(cache_root_from(Some("".into()), exe()), PathBuf::from("/opt/byro"), "an empty override is unset");
        assert_eq!(cache_root_from(None, exe()), PathBuf::from("/opt/byro"));
        assert_eq!(cache_root_from(None, None), PathBuf::from("."));
    }
}
