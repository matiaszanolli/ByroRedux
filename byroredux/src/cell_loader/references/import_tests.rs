//! Worldspace RT-precision predicate + SpeedTree import tests.
//!
//! Extracted from `references/mod.rs`'s inline `mod tests`
//! (#2409 / TD1-006). Contents unchanged.

use super::*;
// Test-only symbols not referenced by production code in this module
// (they'd warn as unused at file scope). #1877 split.
use byroredux_core::string::StringPool;

/// #1495 / REN2-10 — the RT absolute-space precision ceiling guard.
/// Empty cells must not trip (bounds left at ±INF); vanilla-scale
/// extents are clear; a mega-worldspace past 2^20 is flagged with its
/// extent; the bound is inclusive.
#[test]
fn worldspace_extent_ceiling_predicate() {
    // Empty cell — bounds never accumulated, still ±INF → None.
    assert_eq!(
        worldspace_extent_over_rt_ceiling(
            Vec3::splat(f32::INFINITY),
            Vec3::splat(f32::NEG_INFINITY),
        ),
        None,
    );
    // Vanilla exterior corner (~±233k, Skyrim Tamriel) — clear.
    assert_eq!(
        worldspace_extent_over_rt_ceiling(Vec3::splat(-233_000.0), Vec3::splat(233_000.0),),
        None,
    );
    // Mega-worldspace past 2^20 — flagged, returns the max |coord|.
    assert_eq!(
        worldspace_extent_over_rt_ceiling(
            Vec3::new(-1_200_000.0, 0.0, 0.0),
            Vec3::new(50.0, 50.0, 50.0),
        ),
        Some(1_200_000.0),
    );
    // Inclusive at the ceiling itself.
    assert!(worldspace_extent_over_rt_ceiling(
        Vec3::ZERO,
        Vec3::splat(RT_ABSOLUTE_PRECISION_CEILING),
    )
    .is_some());
}

/// Minimal vanilla-shaped `.spt` byte stream: 20-byte magic + one
/// section marker tag + an out-of-range u32 sentinel so the walker
/// stops cleanly at the geometry-tail boundary.
fn minimal_spt_bytes() -> Vec<u8> {
    // Magic header (`E8 03 00 00 0C 00 00 00 __IdvSpt_02_`).
    let mut bytes = vec![0xE8, 0x03, 0x00, 0x00, 0x0C, 0x00, 0x00, 0x00];
    bytes.extend_from_slice(b"__IdvSpt_02_");
    // Single bare-marker tag (`1002` is in the bare set).
    bytes.extend_from_slice(&1002u32.to_le_bytes());
    // Tail sentinel — out-of-range u32 so the walker stops cleanly.
    bytes.extend_from_slice(&0x4E25u32.to_le_bytes());
    bytes
}

/// #3076 regression — the renderable placeholder mesh, not its
/// non-renderable placement root, carries the billboard mode.
#[test]
fn parse_and_import_spt_surfaces_billboard_mode_on_mesh() {
    let bytes = minimal_spt_bytes();
    let mut pool = StringPool::new();
    let cached = parse_and_import_spt(&bytes, "trees\\test.spt", None, &mut pool, None)
        .expect("minimal spt parses through the importer");
    assert_eq!(cached.placement_root_billboard, None);
    assert_eq!(cached.meshes.len(), 1, "single placeholder quad");
    assert_eq!(cached.meshes[0].billboard_mode, Some(5));
}

#[test]
fn parse_and_import_spt_does_not_guess_wind_from_tree_cnam() {
    let bytes = minimal_spt_bytes();
    let tree = byroredux_plugin::esm::records::TreeRecord {
        canopy_params: vec![2.0, 0.25],
        ..Default::default()
    };
    let mut pool = StringPool::new();
    let cached = parse_and_import_spt(&bytes, "trees\\windy.spt", Some(&tree), &mut pool, None)
        .expect("minimal spt parses through the importer");
    assert_eq!(cached.speedtree_wind, Some((1.0, 0.0)));
}

#[test]
fn malformed_spt_still_produces_placeholder() {
    let mut pool = StringPool::new();
    let cached = parse_and_import_spt(b"not an spt", "trees\\broken.spt", None, &mut pool, None)
        .expect("parse failure must degrade to a placeholder");
    assert_eq!(cached.meshes.len(), 1);
    assert_eq!(cached.meshes[0].billboard_mode, Some(5));
}

/// #1820 / SPT-NEW-01 — pins the logged sanity check
/// `parse_and_import_spt` now computes via `detect_variant`. The
/// call itself can't be observed without a log-capturing dependency
/// (none exists in this workspace), so this asserts the value the
/// production code path would log for the same fixture bytes
/// `parse_and_import_spt_surfaces_billboard_mode_on_mesh`
/// exercises above — a vanilla `__IdvSpt_02_`-prefixed stream
/// resolves to `V5Fnv` per `detect_variant`'s documented default.
#[test]
fn minimal_spt_fixture_detects_as_v5fnv_variant() {
    let bytes = minimal_spt_bytes();
    assert_eq!(
        byroredux_spt::detect_variant(&bytes),
        byroredux_spt::SpeedTreeVariant::V5Fnv,
        "the same bytes parse_and_import_spt's logged sanity check \
         receives must resolve to V5Fnv, matching MAGIC_HEAD's \
         documented default",
    );
}

/// #3528 — `TREE.ICON` is a bare filename on 100 % of vanilla TREE records,
/// so the engine's `textures\` prefix alone produced `textures\<Name>.dds`,
/// which exists in no shipped archive: the placeholder billboard's only
/// visible surface always fell through to the magenta checker.
///
/// Pins the probe order against a stubbed archive rather than a real BSA, so
/// it runs in the ordinary suite. The corpus half —
/// `vanilla_tree_icons_all_resolve` below — is env-gated.
#[test]
fn tree_icon_resolves_bare_filenames_under_the_measured_directory() {
    use super::import::resolve_tree_icon_path;

    // The vanilla layout: the leaf card lives under `trees\leaves\`.
    let archive = |p: &str| p.eq_ignore_ascii_case("trees\\leaves\\WhiteOakLeaves01.dds");
    assert_eq!(
        resolve_tree_icon_path("WhiteOakLeaves01.dds", archive),
        "trees\\leaves\\WhiteOakLeaves01.dds",
        "a bare ICON must be resolved against the real archive layout"
    );

    // `billboards\` is the second probe — reached only when `leaves\` misses.
    let billboards_only = |p: &str| p == "trees\\billboards\\EuonymusBush01.dds";
    assert_eq!(
        resolve_tree_icon_path("EuonymusBush01.dds", billboards_only),
        "trees\\billboards\\EuonymusBush01.dds"
    );

    // …but never in preference to it. 10 of the 93 vanilla ICONs exist in
    // BOTH directories, and `leaves\` is the one that covers all 93.
    let both = |p: &str| {
        p == "trees\\leaves\\EuonymusBush01.dds" || p == "trees\\billboards\\EuonymusBush01.dds"
    };
    assert_eq!(
        resolve_tree_icon_path("EuonymusBush01.dds", both),
        "trees\\leaves\\EuonymusBush01.dds",
        "leaves\\ must win when both exist — it is the directory the whole \
         corpus resolves under"
    );

    // An ICON that already resolves verbatim is authoritative and untouched.
    let verbatim = |p: &str| p == "textures\\custom\\MyLeaf.dds";
    assert_eq!(
        resolve_tree_icon_path("textures\\custom\\MyLeaf.dds", verbatim),
        "textures\\custom\\MyLeaf.dds"
    );

    // An authored path that misses is reported as-authored, not re-prefixed
    // into a path its author never wrote.
    assert_eq!(
        resolve_tree_icon_path("some\\mod\\Leaf.dds", |_| false),
        "some\\mod\\Leaf.dds"
    );

    // Nothing anywhere → the ICON passes through and the caller renders the
    // checker, exactly as before the fix. No silent path invention.
    assert_eq!(
        resolve_tree_icon_path("Missing.dds", |_| false),
        "Missing.dds"
    );
}

/// #3528 corpus gate — every vanilla `TREE.ICON` must resolve to a real
/// archive entry through `resolve_tree_icon_path`.
///
/// Env-gated and skipped when the data is absent, matching
/// `crates/spt/tests/parse_real_spt.rs`'s convention. This is the test that
/// would have caught the original defect: the pre-fix resolver returned the
/// bare ICON, which resolves for 0 of 93.
///
/// ```bash
/// BYROREDUX_FNV_DATA=".../Fallout New Vegas/Data" \
/// BYROREDUX_FO3_DATA=".../Fallout 3 goty/Data" \
/// BYROREDUX_OBL_DATA=".../Oblivion/Data" \
///     cargo test -p byroredux vanilla_tree_icons_all_resolve -- --nocapture
/// ```
#[test]
fn vanilla_tree_icons_all_resolve() {
    use super::import::resolve_tree_icon_path;

    let games: [(&str, &str, &str, &str); 3] = [
        (
            "FNV",
            "BYROREDUX_FNV_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data",
            "FalloutNV.esm",
        ),
        (
            "FO3",
            "BYROREDUX_FO3_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data",
            "Fallout3.esm",
        ),
        (
            "OBL",
            "BYROREDUX_OBL_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data",
            "Oblivion.esm",
        ),
    ];

    let mut checked = 0usize;
    let mut skipped = 0usize;
    for (label, env, fallback, esm) in games {
        let dir = std::env::var(env)
            .ok()
            .map(std::path::PathBuf::from)
            .filter(|p| p.exists())
            .or_else(|| {
                let p = std::path::PathBuf::from(fallback);
                p.exists().then_some(p)
            });
        let Some(dir) = dir else {
            eprintln!("[{label}] skip: {env} unset and fallback missing");
            skipped += 1;
            continue;
        };

        // Index every texture archive in the game's Data dir.
        let mut archives = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for entry in rd.flatten() {
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("bsa") {
                    if let Ok(a) = byroredux_bsa::BsaArchive::open(&p) {
                        archives.push(a);
                    }
                }
            }
        }
        let probe = |path: &str| {
            let normalised = crate::asset_provider::normalize_texture_path(path);
            archives.iter().any(|a| a.contains(normalised.as_ref()))
        };

        let Ok(bytes) = std::fs::read(dir.join(esm)) else {
            eprintln!("[{label}] skip: {esm} unreadable");
            skipped += 1;
            continue;
        };
        let index = byroredux_plugin::esm::records::parse_esm(&bytes).expect("parse esm");

        let mut icons: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for tree in index.trees.values() {
            if !tree.leaf_texture.is_empty() {
                icons.insert(tree.leaf_texture.clone());
            }
        }
        assert!(
            !icons.is_empty(),
            "[{label}] no TREE.ICON values found — the census fixture is broken"
        );

        let mut unresolved = Vec::new();
        for icon in &icons {
            let resolved = resolve_tree_icon_path(icon, probe);
            if !probe(&resolved) {
                unresolved.push(icon.clone());
            }
            checked += 1;
        }
        assert!(
            unresolved.is_empty(),
            "[{label}] {} of {} vanilla TREE.ICON values resolve to no archive \
             entry: {unresolved:?}",
            unresolved.len(),
            icons.len()
        );
        eprintln!(
            "[{label}] all {} unique TREE.ICON values resolve",
            icons.len()
        );
    }

    if skipped == 3 {
        eprintln!("vanilla_tree_icons_all_resolve: no game data available, nothing checked");
    } else {
        assert!(
            checked > 0,
            "at least one game's ICONs must have been checked"
        );
    }
}

/// #3735 — a `.spt` TREE `MODL` is a leading-separator bare filename on
/// 100 % of vanilla records (154 of 154 across Oblivion, FO3 and FNV), and
/// SpeedTree binaries live outside the `meshes\` root. The generic
/// composition therefore built `meshes\\<name>.spt`, which no archive holds,
/// so `extract_mesh` returned `None`, the `.spt` dispatch was never entered,
/// and the REFR was dropped. **No SpeedTree billboard had ever rendered from
/// a cell load.**
///
/// Same shape as `tree_icon_resolves_bare_filenames_under_the_measured_directory`
/// one step earlier in the chain: stubbed archive here, env-gated corpus in
/// `vanilla_tree_models_all_resolve` below.
#[test]
fn tree_model_resolves_leading_separator_bare_names_under_the_measured_directory() {
    use super::import::resolve_spt_model_path;

    // The vanilla shape and layout, verbatim from the census.
    let archive = |p: &str| p == "trees\\WhiteOak01.spt";
    assert_eq!(
        resolve_spt_model_path("\\WhiteOak01.spt", archive),
        "trees\\WhiteOak01.spt",
        "a leading-separator bare MODL must resolve against the real archive \
         layout — `trees\\`, not `meshes\\`"
    );

    // A MODL that already resolves verbatim is authoritative and untouched:
    // mod content naming a real archive path must not be second-guessed.
    let verbatim = |p: &str| p == "trees\\custom\\ModTree.spt";
    assert_eq!(
        resolve_spt_model_path("trees\\custom\\ModTree.spt", verbatim),
        "trees\\custom\\ModTree.spt"
    );

    // The `meshes\`-rooted form still wins when the archive really does keep
    // the asset there — this must not become a regression for anything that
    // resolved before the fix.
    let under_meshes = |p: &str| p == "meshes\\trees\\Pine01.spt";
    assert_eq!(
        resolve_spt_model_path("trees\\Pine01.spt", under_meshes),
        "meshes\\trees\\Pine01.spt"
    );

    // An authored path with a directory that misses everywhere is NOT
    // re-prefixed into a path its author never wrote; the caller reports the
    // miss against the `meshes\`-rooted key, as before.
    assert_eq!(
        resolve_spt_model_path("some\\mod\\Tree.spt", |_| false),
        "meshes\\some\\mod\\Tree.spt"
    );

    // Nothing anywhere → the REFR is skipped, exactly as before the fix. No
    // silent path invention.
    assert_eq!(
        resolve_spt_model_path("\\Missing.spt", |_| false),
        "meshes\\\\Missing.spt"
    );
}

/// #3735 corpus gate — every vanilla `.spt` TREE `MODL` must resolve to a
/// real mesh-archive entry through `resolve_spt_model_path`.
///
/// The sibling of `vanilla_tree_icons_all_resolve`, and the test whose
/// absence let this survive: #3528's guard pinned ICON resolution, and
/// nothing pinned the MODL that feeds `extract_mesh` one step earlier. The
/// pre-fix composition resolves for 0 of 154.
///
/// ```bash
/// BYROREDUX_FNV_DATA=".../Fallout New Vegas/Data" \
/// BYROREDUX_FO3_DATA=".../Fallout 3 goty/Data" \
/// BYROREDUX_OBL_DATA=".../Oblivion/Data" \
///     cargo test -p byroredux vanilla_tree_models_all_resolve -- --nocapture
/// ```
#[test]
fn vanilla_tree_models_all_resolve() {
    use super::import::resolve_spt_model_path;

    let games: [(&str, &str, &str, &str); 3] = [
        (
            "FNV",
            "BYROREDUX_FNV_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data",
            "FalloutNV.esm",
        ),
        (
            "FO3",
            "BYROREDUX_FO3_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data",
            "Fallout3.esm",
        ),
        (
            "OBL",
            "BYROREDUX_OBL_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data",
            "Oblivion.esm",
        ),
    ];

    let mut checked = 0usize;
    let mut skipped = 0usize;
    for (label, env, fallback, esm) in games {
        let dir = std::env::var(env)
            .ok()
            .map(std::path::PathBuf::from)
            .filter(|p| p.exists())
            .or_else(|| {
                let p = std::path::PathBuf::from(fallback);
                p.exists().then_some(p)
            });
        let Some(dir) = dir else {
            eprintln!("[{label}] skip: {env} unset and fallback missing");
            skipped += 1;
            continue;
        };

        let mut archives = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for entry in rd.flatten() {
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("bsa") {
                    if let Ok(a) = byroredux_bsa::BsaArchive::open(&p) {
                        archives.push(a);
                    }
                }
            }
        }
        // The exact-key probe, matching `TextureProvider::has_mesh_exact` —
        // the `.spt` route deliberately skips `normalize_mesh_path`'s
        // `meshes\` rooting, which is what made the correct `trees\` key a
        // guaranteed miss (#3735).
        let probe = |path: &str| archives.iter().any(|a| a.contains(path));

        let Ok(bytes) = std::fs::read(dir.join(esm)) else {
            eprintln!("[{label}] skip: {esm} unreadable");
            skipped += 1;
            continue;
        };
        let index = byroredux_plugin::esm::records::parse_esm(&bytes).expect("parse esm");

        let mut models: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for tree in index.trees.values() {
            if tree.model_path.to_ascii_lowercase().ends_with(".spt") {
                models.insert(tree.model_path.clone());
            }
        }
        assert!(
            !models.is_empty(),
            "[{label}] no `.spt` TREE MODL values found — the census fixture is broken"
        );

        let mut unresolved = Vec::new();
        for model in &models {
            let resolved = resolve_spt_model_path(model, probe);
            if !probe(&resolved) {
                unresolved.push(model.clone());
            }
            checked += 1;
        }
        assert!(
            unresolved.is_empty(),
            "[{label}] {} of {} vanilla `.spt` TREE MODL values resolve to no \
             mesh-archive entry: {unresolved:?}",
            unresolved.len(),
            models.len()
        );
        eprintln!(
            "[{label}] all {} unique `.spt` TREE MODL values resolve",
            models.len()
        );
    }

    if skipped == 3 {
        eprintln!("vanilla_tree_models_all_resolve: no game data available, nothing checked");
    } else {
        assert!(
            checked > 0,
            "at least one game's TREE MODLs must have been checked"
        );
    }
}
