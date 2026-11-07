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

/// #6308 — the loose tier folds keys exactly as the archive readers do
/// (Unicode lowercase), so a non-ASCII override is found by the same
/// lookup that finds the archived copy; two files that fold onto one key
/// resolve deterministically instead of by `read_dir` order.
#[test]
fn loose_keys_fold_like_the_archives() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "textures/Ölfass.dds", b"fass");
    let loose = Archive::open_loose(tmp.path()).expect("loose assets present");
    assert!(loose.contains(r"textures\ölfass.dds"));
    assert!(loose.contains("Textures/ÖLFASS.DDS"));
    assert_eq!(loose.extract(r"textures\ölfass.dds").unwrap(), b"fass");
}

#[cfg(unix)]
#[test]
fn loose_case_variant_duplicates_resolve_to_the_last_path() {
    let tmp = tempfile::tempdir().unwrap();
    // Distinct paths on a case-sensitive filesystem, one archive key.
    write(tmp.path(), "textures/x.dds", b"lower");
    write(tmp.path(), "Textures/x.dds", b"upper");
    if std::fs::read(tmp.path().join("textures/x.dds")).unwrap() != b"lower" {
        return; // case-insensitive filesystem: no duplicate to resolve
    }
    let loose = Archive::open_loose(tmp.path()).expect("loose assets present");
    assert_eq!(loose.list_files().len(), 1);
    // "Textures" < "textures" bytewise, so the lowercase directory is greater.
    assert_eq!(loose.extract(r"textures\x.dds").unwrap(), b"lower");
}

/// #6243 — a symlinked directory is followed, but a link back to an ancestor
/// is not re-walked: two looping links used to cost ~2^40 `read_dir` calls
/// at boot, and one re-indexed its subtree ~40 times under longer keys.
#[cfg(unix)]
#[test]
fn loose_tier_walks_a_symlink_loop_once() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "textures/real/a.dds", b"a");
    write(tmp.path(), "outside/linked.dds", b"linked");
    let textures = tmp.path().join("textures");
    std::os::unix::fs::symlink(&textures, textures.join("loop_a")).unwrap();
    std::os::unix::fs::symlink(&textures, textures.join("loop_b")).unwrap();
    std::os::unix::fs::symlink(tmp.path().join("outside"), textures.join("deployed")).unwrap();

    let loose = Archive::open_loose(tmp.path()).expect("loose assets present");
    assert!(loose.contains(r"textures\real\a.dds"));
    assert!(loose.contains(r"textures\deployed\linked.dds"), "a linked directory is followed");
    assert!(!loose.contains(r"textures\loop_a\real\a.dds"), "a link to an ancestor is not re-walked");
    assert_eq!(loose.list_files().len(), 2, "{:?}", loose.list_files());
}

/// #6483 — two links to one directory are two legitimate paths, and the game
/// resolves both. The old global visited set indexed the files under whichever
/// link `read_dir` yielded first and dropped the other, silently.
#[cfg(unix)]
#[test]
fn loose_tier_indexes_a_directory_linked_from_two_places_under_both_keys() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "mods/x/tex/shared.dds", b"shared");
    write(tmp.path(), "textures/own.dds", b"own");
    let textures = tmp.path().join("textures");
    std::os::unix::fs::symlink(tmp.path().join("mods/x"), textures.join("a")).unwrap();
    std::os::unix::fs::symlink(tmp.path().join("mods/x"), textures.join("b")).unwrap();

    let loose = Archive::open_loose(tmp.path()).expect("loose assets present");
    assert!(loose.contains(r"textures\a\tex\shared.dds"), "{:?}", loose.list_files());
    assert!(loose.contains(r"textures\b\tex\shared.dds"), "{:?}", loose.list_files());
    assert!(loose.contains(r"textures\own.dds"));
    assert_eq!(loose.list_files().len(), 3, "{:?}", loose.list_files());
}

/// #6483 — a loop that runs through a SECOND directory (a -> b -> a) is cut at the
/// ancestor, not at "already seen", so both directories still index their files.
#[cfg(unix)]
#[test]
fn loose_tier_cuts_a_two_directory_loop_at_the_ancestor() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "textures/a/fa.dds", b"a");
    write(tmp.path(), "textures/b/fb.dds", b"b");
    let textures = tmp.path().join("textures");
    std::os::unix::fs::symlink(textures.join("b"), textures.join("a/to_b")).unwrap();
    std::os::unix::fs::symlink(textures.join("a"), textures.join("b/to_a")).unwrap();

    // Each directory is reached directly and once through the other's link; the link back
    // (b/to_a under a/to_b, a/to_b under b/to_a) is the loop and is cut. The budget is not involved.
    let keys = super::super::archive::loose_keys_bounded(tmp.path(), 1_000);
    assert_eq!(
        keys,
        [
            r"textures\a\fa.dds",
            r"textures\a\to_b\fb.dds",
            r"textures\b\fb.dds",
            r"textures\b\to_a\fa.dds",
        ]
    );
}

/// #6483 — the ancestor stack alone does not bound a chain of doubled links
/// (d0 links twice to d1, d1 twice to d2, ...: 2^n walks, no loop in it); the
/// directory budget does, and the index it returns is partial, not a hang.
#[cfg(unix)]
#[test]
fn loose_tier_bounds_a_chain_of_doubled_links() {
    let tmp = tempfile::tempdir().unwrap();
    let depth = 24; // 2^24 walks unbounded
    for level in 0..=depth {
        write(tmp.path(), &format!("mods/d{level}/f{level}.dds"), b"x");
    }
    for level in 0..depth {
        for link in ["l0", "l1"] {
            std::os::unix::fs::symlink(
                tmp.path().join(format!("mods/d{}", level + 1)),
                tmp.path().join(format!("mods/d{level}/{link}")),
            )
            .unwrap();
        }
    }
    std::fs::create_dir_all(tmp.path().join("textures")).unwrap();
    std::os::unix::fs::symlink(tmp.path().join("mods/d0"), tmp.path().join("textures/root")).unwrap();

    let keys = super::super::archive::loose_keys_bounded(tmp.path(), 500);
    // `textures` plus 499 chain directories use the 500-directory budget; each chain directory holds
    // one file (the links are directories, not files), so exactly 499 files are indexed and the walk
    // ends instead of making 2^24 passes.
    assert_eq!(keys.len(), 499, "{keys:?}");
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

/// #6330 — Starfield's `geometries\<hash>.mesh` external mesh data is a loose
/// asset root too, and the mesh provider serves it unprefixed.
#[test]
fn loose_tier_serves_starfield_geometries() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "Geometries/ab12/cd34.mesh", b"mesh body");
    let loose = Archive::open_loose(tmp.path()).expect("loose assets present");
    assert!(loose.contains(r"geometries\ab12\cd34.mesh"));

    let textures = build_texture_provider(&esm_args(tmp.path()));
    assert_eq!(textures.extract_mesh(r"geometries\ab12\cd34.mesh").unwrap(), b"mesh body");
}

/// #6330 — a loose `materials\materialsbeta.cdb` (and a Creation-namespaced one)
/// is discovered and registered by the material provider, and a directory
/// source re-opens as the loose tier so its index can be built.
#[test]
fn a_loose_cdb_is_discovered_and_reopens_by_its_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let cdb = super::starfield_mat::minimal_cdb_bytes();
    write(tmp.path(), "Materials/materialsbeta.cdb", &cdb);
    write(tmp.path(), "materials/creations/mymod/materialsbeta.cdb", &cdb);

    let materials = build_material_provider(&esm_args(tmp.path()));
    let mut sources: Vec<&str> = materials.sf_cdb_sources.iter().map(|(_, inner)| inner.as_str()).collect();
    sources.sort_unstable();
    assert_eq!(
        sources,
        [r"materials\creations\mymod\materialsbeta.cdb", r"materials\materialsbeta.cdb"],
        "both loose CDBs register"
    );

    let (source, _) = &materials.sf_cdb_sources[0];
    let (reopened, _) = Archive::open_shared(source).expect("a directory source re-opens as the loose tier");
    assert_eq!(reopened.extract(r"materials\materialsbeta.cdb").unwrap(), cdb);
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
