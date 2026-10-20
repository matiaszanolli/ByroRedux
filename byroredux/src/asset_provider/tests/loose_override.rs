//! #6200 — loose files under the game's `Data` directory override every
//! archive: mod and user assets win.
//!
//! The synthetic tests drive the loose tier against a temp `Data` tree. The
//! end-to-end precedence half needs a real archive carrying the same path,
//! so it is `#[ignore]`-gated against the installed FO4 `Data/`, same
//! convention as `archive_precedence.rs`. Run with:
//!
//!   cargo test -p byroredux --bin byroredux loose_override -- --ignored

use super::super::*;

fn write(root: &std::path::Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn esm_args(data: &std::path::Path) -> Vec<String> {
    vec![
        "--esm".to_owned(),
        data.join("Test.esm").to_string_lossy().into_owned(),
    ]
}

#[test]
fn loose_data_dir_is_the_esm_directory() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(
        loose_data_dir(&esm_args(tmp.path())),
        Some(tmp.path().to_path_buf())
    );
    // A bare plugin name lives in the working directory.
    assert_eq!(
        loose_data_dir(&["--esm".to_owned(), "Test.esm".to_owned()]),
        Some(std::path::PathBuf::from("."))
    );
    assert_eq!(
        loose_data_dir(&["--mesh".to_owned(), "a.nif".to_owned()]),
        None
    );
}

/// Archive-style keys match regardless of the install's case or separator,
/// only the asset roots archives use are indexed, and an install with no
/// loose assets opens no tier at all.
#[test]
fn loose_tier_indexes_asset_roots_by_archive_key() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "Textures/Landscape/Dirt01_D.dds", b"loose dirt");
    write(tmp.path(), "meshes/clutter/cup.nif", b"cup");
    write(tmp.path(), "Materials/landscape/dirt.bgsm", b"bgsm");
    write(tmp.path(), "Sound/fx/step.wav", b"wav");
    write(tmp.path(), "readme.txt", b"txt");

    let loose = Archive::open_loose(tmp.path()).expect("loose assets present");
    assert!(loose.contains(r"textures\landscape\dirt01_d.dds"));
    assert!(loose.contains("TEXTURES/Landscape/Dirt01_D.DDS"));
    assert_eq!(
        loose.extract(r"textures\landscape\dirt01_d.dds").unwrap(),
        b"loose dirt"
    );
    assert_eq!(loose.declared_size(r"meshes\clutter\cup.nif").unwrap(), 3);
    assert!(loose.contains(r"materials\landscape\dirt.bgsm"));
    assert!(!loose.contains(r"sound\fx\step.wav"));
    assert!(!loose.contains("readme.txt"));
    assert_eq!(
        loose.extract(r"textures\missing.dds").unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );

    let empty = tempfile::tempdir().unwrap();
    write(empty.path(), "Sound/fx/step.wav", b"wav");
    assert!(Archive::open_loose(empty.path()).is_none());
}

/// Both providers pick the tier up from `--esm` alone.
#[test]
fn providers_serve_loose_assets() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "textures/landscape/dirt01_d.dds", b"loose dirt");
    write(tmp.path(), "meshes/clutter/cup.nif", b"cup");
    write(tmp.path(), "materials/landscape/dirt.bgsm", b"bgsm");
    let args = esm_args(tmp.path());

    let textures = build_texture_provider(&args);
    // LTEX-style path, relative to the `textures\` root.
    assert_eq!(
        textures.extract(r"landscape\dirt01_d.dds").unwrap(),
        b"loose dirt"
    );
    assert_eq!(
        textures.extract_mesh(r"meshes\clutter\cup.nif").unwrap(),
        b"cup"
    );

    let materials = build_material_provider(&args);
    assert_eq!(
        materials
            .extract_from_archives(r"materials\landscape\dirt.bgsm")
            .unwrap(),
        b"bgsm"
    );
}

/// A loose copy of a path the vanilla texture archive also carries wins over
/// the archive, whatever order the archives were listed in.
#[test]
#[ignore = "needs Fallout 4 game data on disk"]
fn loose_texture_overrides_the_archived_copy() {
    let archive = std::path::PathBuf::from(
        std::env::var_os("BYROREDUX_FO4_DATA")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data".into()),
    )
    .join("Fallout4 - Textures1.ba2");
    if !archive.is_file() {
        eprintln!("skipping: {} not found", archive.display());
        return;
    }
    let key = r"textures\landscape\ground\commonwealthdefault01_d.dds";
    let vanilla = Archive::open(&archive.to_string_lossy()).unwrap();
    assert_ne!(
        vanilla
            .extract(key)
            .expect("vanilla FO4 ships the default ground"),
        b"mod dirt"
    );

    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), &key.replace('\\', "/"), b"mod dirt");
    let mut args = esm_args(tmp.path());
    args.extend([
        "--textures-bsa".to_owned(),
        archive.to_string_lossy().into_owned(),
    ]);
    let provider = build_texture_provider(&args);
    assert_eq!(provider.extract(key).unwrap(), b"mod dirt");
}
