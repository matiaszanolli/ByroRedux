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

/// Whether `name` is a single bare path component that `Path::join` keeps under its base: not
/// empty, not `.` or `..`, and free of `\\`, `/` and `:`.
///
/// #5016 — checked by string, not `Path`, so the rule is the same on every host:
/// `Path::is_absolute` is false on Windows for rooted-prefixless `\x` and prefixed-rootless
/// `C:x`, yet `join` with either escapes the base, and on Linux `..\..\x` is one `Normal`
/// component. A Windows file or directory name cannot contain `\`, `/` or `:`, so rejecting all
/// three loses no real name. Shared by Steam's `installdir` check and the script path resolver
/// (#7142), which had refused `..` but not a drive prefix.
pub fn is_bare_component(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':'])
}

/// `path` without leading separators and without one leading `Data\` folder (either separator, any case): the
/// game's `Data` directory is the root every asset key is relative to, so an authored `Data\Meshes\x.nif` or
/// `\textures\x.dds` names the same file as `meshes\x.nif` / `textures\x.dds` (LMC-AST-005). No installed game's
/// archives hold a `data\` folder at their root or under `meshes\` (FNV, Skyrim SE and Fallout 4: 1.03 million
/// entries, 2026-11-23), so the strip cannot shadow a real path. Borrows.
pub fn strip_data_root(path: &str) -> &str {
    let trimmed = path.trim_start_matches(['\\', '/']);
    let bytes = trimmed.as_bytes();
    if bytes.len() > 5 && bytes[..4].eq_ignore_ascii_case(b"data") && matches!(bytes[4], b'\\' | b'/') {
        trimmed[5..].trim_start_matches(['\\', '/'])
    } else {
        trimmed
    }
}

/// Prepend `meshes\` to a NIF path when the input doesn't already
/// start with that segment (case-insensitive, accepts either
/// separator). `MODL` sub-records on RACE / NPC_ / ARMO records are
/// authored relative to the `meshes\` root; the BSA layer stores the
/// full prefix. Allocation only fires when the prefix is missing —
/// already-prefixed paths borrow. The single home of "prepend `meshes\`
/// unless present" (#6956): [`archive_mesh_path`] and
/// [`canonical_mesh_key`] are its owned forms. #7145 — moved here from the
/// binary's `asset_provider/archive.rs` (which re-exports all three) so the
/// renderer's mesh cache keys through the same rule, `geometries\` exemption
/// included, instead of a copy of its own.
pub fn normalize_mesh_path(path: &str) -> std::borrow::Cow<'_, str> {
    // LMC-AST-005 — a leading separator or `Data\` folder is not part of the archive key.
    let path = strip_data_root(path);
    // #1292 — Starfield content-addressed BSGeometry external `.mesh`
    // companion files live at `geometries\<hash>.mesh` (NO `meshes\`
    // prefix). The importer at `crates/nif/src/import/mesh/bs_geometry.rs`
    // composes the canonical path before calling the resolver; without
    // this gate the normaliser silently prepended `meshes\` and turned
    // every Starfield poster / architecture / set-dressing lookup into
    // a guaranteed miss → 99.7% spawn-rate failure on Cydonia.
    if starts_with_root(path, "meshes") || starts_with_root(path, "geometries") {
        return std::borrow::Cow::Borrowed(path);
    }
    std::borrow::Cow::Owned(format!(r"meshes\{}", path))
}

/// Whether `path` starts with the folder `root` and a separator (`\` or `/`), `root`'s case
/// ignored. Compared as bytes, so a name whose cut would land inside a multi-byte character
/// cannot panic (#3391). The root test [`normalize_mesh_path`] and the Starfield `geometries\`
/// composer in `crates/nif` share (#7145).
pub fn starts_with_root(path: &str, root: &str) -> bool {
    let (bytes, root) = (path.as_bytes(), root.as_bytes());
    bytes.len() > root.len()
        && bytes[..root.len()].eq_ignore_ascii_case(root)
        && matches!(bytes[root.len()], b'\\' | b'/')
}

/// `path` as an archive mesh key: leading separators trimmed, `/` turned into `\`, and
/// `meshes\` prefixed per [`normalize_mesh_path`] (case kept).
pub fn archive_mesh_path(path: &str) -> String {
    let path = path.trim_start_matches(['\\', '/']).replace('/', "\\");
    normalize_mesh_path(&path).into_owned()
}

/// The case fold every cache keyed by an archive path uses: Unicode lower-casing, as
/// `byroredux_bsa::normalize_path` folds every archive key (#7168, #7180). An ASCII-only fold kept
/// `Ä.nif` and `ä.nif` apart in a cache while the archives resolved both to one file, so it was
/// read, parsed and uploaded twice. The two crates do not depend on each other; the binary pins
/// that this and the archives' fold agree (through [`canonical_mesh_key`]).
pub fn fold_case(path: &str) -> String {
    path.to_lowercase()
}

/// [`archive_mesh_path`], case-folded ([`fold_case`]): the key a lookup table or cache shares
/// between every producer of it.
pub fn canonical_mesh_key(path: &str) -> String {
    archive_mesh_path(&fold_case(path))
}

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

    /// #7168 — a mesh key folds case beyond ASCII, so two spellings of one archive file share it.
    #[test]
    fn mesh_keys_fold_unicode_case() {
        assert_eq!(canonical_mesh_key("Ä.NIF"), "meshes\\ä.nif");
        assert_eq!(canonical_mesh_key("Meshes/Ünter/Ä.nif"), canonical_mesh_key("meshes\\ünter\\ä.nif"));
        assert_eq!(fold_case("ÄBC\\Straße.DDS"), "äbc\\straße.dds");
    }

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

    /// LMC-AST-005 — a leading separator or `Data\` folder is not part of an asset key, for meshes as for textures.
    #[test]
    fn a_leading_separator_or_data_folder_is_not_part_of_the_key() {
        assert_eq!(strip_data_root(r"Data\Meshes\x.nif"), r"Meshes\x.nif");
        assert_eq!(strip_data_root(r"\data/textures\x.dds"), r"textures\x.dds");
        assert_eq!(strip_data_root("/x.nif"), "x.nif");
        assert_eq!(strip_data_root(r"database\x.nif"), r"database\x.nif", "only a whole `data` folder");
        assert_eq!(strip_data_root(r"meshes\data\x.nif"), r"meshes\data\x.nif", "only a leading one");
        assert_eq!(normalize_mesh_path(r"Data\Meshes\Armor\x.nif"), r"Meshes\Armor\x.nif");
        assert_eq!(normalize_mesh_path(r"\Armor\x.nif"), r"meshes\Armor\x.nif");
        assert_eq!(normalize_mesh_path(r"data\Armor\x.nif"), r"meshes\Armor\x.nif");
        assert!(matches!(normalize_mesh_path(r"Data\Meshes\x.nif"), std::borrow::Cow::Borrowed(_)));
    }
}
