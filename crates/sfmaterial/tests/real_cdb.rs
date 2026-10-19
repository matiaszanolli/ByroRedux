//! Real-data smoke test against vanilla `materialsbeta.cdb` from
//! `Starfield - Materials.ba2`. `#[ignore]`-gated by Starfield install.
//!
//! Run with:
//! ```
//! BYROREDUX_STARFIELD_DATA="/path/to/Starfield/Data" \
//!     cargo test -p byroredux-sfmaterial --test real_cdb -- --ignored --nocapture
//! ```
//!
//! The vanilla base-game `materialsbeta.cdb` contains ~1.44M instances. This
//! test deliberately uses the nested streaming validator, so only the schema
//! and string table stay live. Do not change it back to
//! `ComponentDatabaseFile::parse`: materialising the full generic value tree
//! has measured at ~9.19 GB peak RSS (#4274 / SF-D3-2026-09-11-03).

use byroredux_bsa::Ba2Archive;
use byroredux_sfmaterial::{ComponentDatabaseFile, MaterialIndex, ParseLimits};
use std::path::PathBuf;

/// #3850 — the strict lane for real-data tests.
///
/// `BYROREDUX_REQUIRE_GAME_DATA=1` turns an absent corpus into a hard
/// failure instead of a silent libtest `ok`. Without it the `--ignored`
/// lane — the only lane these `#[ignore]`d tests ever execute in —
/// records a pass for a test that never touched a byte of game data, so
/// a green run is not evidence of anything. This is the Rust-side
/// counterpart of the shell gates' "explicit SKIP with exit code 77,
/// never a pass" rule (`docs/smoke-tests/README.md`, #3003).
#[track_caller]
fn require_game_data(env_var: &str, tried: &std::path::Path) {
    if std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok_and(|v| v != "0") {
        panic!(
            "BYROREDUX_REQUIRE_GAME_DATA is set, but no game data was found: \
             {env_var} is unset (or names a non-directory) and the default \
             {tried:?} is not a directory"
        );
    }
}

fn data_dir() -> Option<PathBuf> {
    if let Some(v) = std::env::var("BYROREDUX_STARFIELD_DATA")
        .ok()
        .filter(|s| !s.is_empty())
    {
        let p = PathBuf::from(&v);
        if p.exists() {
            return Some(p);
        }
        // #3850: an explicitly-set override is BINDING — never silently
        // substitute the hardcoded dev-machine path.
        panic!("BYROREDUX_STARFIELD_DATA points to {v:?}, which is not a directory");
    }
    let p = PathBuf::from("/mnt/data/SteamLibrary/steamapps/common/Starfield/Data");
    if p.exists() {
        return Some(p);
    }
    require_game_data("BYROREDUX_STARFIELD_DATA", &p);
    None
}

#[test]
#[ignore = "needs Starfield game data on disk"]
fn parse_vanilla_materialsbeta_cdb() {
    let Some(data) = data_dir() else {
        return;
    };
    let ba2 = Ba2Archive::open(data.join("Starfield - Materials.ba2")).expect("open materials BA2");
    let bytes = ba2
        .extract("materials\\materialsbeta.cdb")
        .expect("extract cdb");
    eprintln!("[sfmaterial] extracted {} bytes", bytes.len());
    eprintln!(
        "[sfmaterial] validating with ParseLimits::unlimited() — nested \
         values are consumed without materialising the 1.44M-instance tree"
    );

    let info =
        ComponentDatabaseFile::validate_instances_with_limits(&bytes, ParseLimits::unlimited())
            .expect("validate CDB");
    eprintln!(
        "[sfmaterial] streamed: {} classes / {} top-level values",
        info.class_count, info.value_count
    );

    // #4665 (PAR-D4-2026-09-21-03) — the counts are pinned EXACTLY (the
    // audit's measured 97 classes / 1,438,780 values), not floored: the
    // CDB is versioned content shipped with the game, so a change is a
    // format event worth failing on, not noise to absorb.
    assert_eq!(info.class_count, 97, "vanilla CDB class count");
    assert_eq!(info.value_count, 1_438_780, "vanilla CDB top-level values");
}

/// #3398 Phase 2 — the compact index over the vanilla base CDB: build
/// (streaming, never the ~9.19 GB full parse) and resolve one material
/// the 2026-09-30 probe verified field-by-field:
/// `Materials\Actors\BipedA\BipedA_Nightmare\BipedA_Nightmare.mat`
/// resolves to a texture set carrying color/normal/rough/metal/ao slots.
#[test]
#[ignore = "needs Starfield game data on disk"]
fn material_index_resolves_a_vanilla_material() {
    let Some(data) = data_dir() else {
        return;
    };
    let ba2 = Ba2Archive::open(data.join("Starfield - Materials.ba2")).expect("open materials BA2");
    let bytes = ba2
        .extract("materials\\materialsbeta.cdb")
        .expect("extract cdb");

    let started = std::time::Instant::now();
    let index = MaterialIndex::build(&bytes).expect("index the vanilla CDB");
    let status_kb = |field: &str| -> u64 {
        std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with(field))
                    .and_then(|l| l.split_whitespace().nth(1).and_then(|v| v.parse().ok()))
            })
            .unwrap_or(0)
    };
    let build_secs = started.elapsed().as_secs_f32();
    let hwm_kb = status_kb("VmHWM:");
    let with_blob_kb = status_kb("VmRSS:");
    // #5291 — what the retained index costs once the inflated blob is gone
    // (the production shape: `cdb_material_index` drops the bytes after the
    // build). RSS is what the allocator still holds, so this is an upper
    // bound on the index itself.
    drop(bytes);
    let resident_kb = status_kb("VmRSS:");
    eprintln!(
        "[sfmaterial] indexed {} keyed materials in {:.1}s, HWM {} MB, RSS {} MB with the \
         inflated blob / {} MB after dropping it (full parse measures 9.19 GB)",
        index.material_count(),
        build_secs,
        hwm_kb / 1024,
        with_blob_kb / 1024,
        resident_kb / 1024,
    );
    // Measured 2026-09-30: the full Objects table keys 500,403 rows
    // (material roots + layers + texture sets). ~250-470 MB HWM / 1.6-2.2 s (re-measured 2026-10-05: 244 MB / 1.6 s) on
    // this run, versus 9.19 GB for the full generic parse.
    assert_eq!(index.material_count(), 500_403, "vanilla CDB key count");

    let mat = index
        .lookup("Materials\\Actors\\BipedA\\BipedA_Nightmare\\BipedA_Nightmare.mat")
        .expect("the probe-verified material must resolve");
    let slot = |want: u8| {
        mat.textures
            .iter()
            .find(|(s, _)| *s == want)
            .map(|(_, p)| p.to_ascii_lowercase())
    };
    let assert_slot = |want: u8, suffix: &str| {
        let got = slot(want).unwrap_or_default();
        assert!(
            got.replace('/', "\\").ends_with(suffix),
            "slot {want} should carry *{suffix}, got {got:?}"
        );
    };
    assert_slot(byroredux_sfmaterial::SLOT_COLOR, "bipeda_nightmare_color.dds");
    assert_slot(byroredux_sfmaterial::SLOT_NORMAL, "bipeda_nightmare_normal.dds");
    assert_slot(byroredux_sfmaterial::SLOT_ROUGHNESS, "bipeda_nightmare_rough.dds");
    assert_slot(byroredux_sfmaterial::SLOT_METALNESS, "bipeda_nightmare_metal.dds");
    assert_slot(byroredux_sfmaterial::SLOT_AMBIENT_OCCLUSION, "bipeda_nightmare_ao.dds");
}

/// #5502 — `ObjectInfo.Parent` inheritance on real data. Both materials
/// author neither field themselves; pre-fix the lookup read them as absent
/// (`IsGlass`/emissive `Enabled` came back `None` on all but 4 of the
/// 10,152 NIF-referenced materials), so the #5277 / #5283 translations
/// fired on nothing. Census over the 10,152 hits after the fix (paths from
/// `sf_matpath_dump` over Meshes01): glass 4 -> 217, emissive Enabled
/// 0 -> 619 (337 with an emissive map).
#[test]
#[ignore = "needs Starfield game data on disk"]
fn material_index_resolves_inherited_glass_and_emissive_settings() {
    let Some(data) = data_dir() else {
        return;
    };
    let ba2 = Ba2Archive::open(data.join("Starfield - Materials.ba2")).expect("open materials BA2");
    let bytes = ba2
        .extract("materials\\materialsbeta.cdb")
        .expect("extract cdb");
    let index = MaterialIndex::build(&bytes).expect("index the vanilla CDB");

    let glass = index
        .lookup("Materials\\Architecture\\Catwalks\\GlassCleanFrosted02_Catwalks.mat")
        .expect("probe-verified material");
    assert_eq!(glass.is_glass, Some(true), "IsGlass is inherited down the Parent chain");

    let emissive = index
        .lookup("Materials\\Actors\\MantaA\\MantaA_Sentinel\\MantaA_Sentinel.mat")
        .expect("probe-verified material");
    assert_eq!(emissive.emissive_enabled, Some(true), "Enabled is inherited");
    assert_eq!(emissive.luminous_emittance, Some(120.0));
    assert!(
        emissive.textures.iter().any(|(slot, _)| *slot == byroredux_sfmaterial::SLOT_EMISSIVE),
        "the emissive map binds"
    );
}

/// The DLC CDBs are full near-copies (SFBGS007 measured 2026-09-30:
/// 500,385 keys vs the base's 500,403, resolving the same corpus), so a
/// last-wins overlay across discovered CDBs in load order is the correct
/// merge shape — this pins that both CDBs resolve the same material.
#[test]
#[ignore = "needs Starfield game data on disk"]
fn material_index_dlc_cdb_resolves_the_base_corpus() {
    let Some(data) = data_dir() else {
        return;
    };
    let dlc = data.join("SFBGS007 - Main.ba2");
    if !dlc.exists() {
        // #5326 — an absent DLC under the strict lane is a failure, not a
        // silent pass (#3850).
        require_game_data("BYROREDUX_STARFIELD_DATA (SFBGS007 - Main.ba2)", &dlc);
        eprintln!("[sfmaterial] SKIP: {} not installed", dlc.display());
        return;
    }
    let ba2 = Ba2Archive::open(&dlc).expect("open DLC main BA2");
    let bytes = ba2
        .extract("materials\\creations\\SFBGS007\\materialsbeta.cdb")
        .expect("extract DLC cdb");
    let index = MaterialIndex::build(&bytes).expect("index the DLC CDB");
    eprintln!(
        "[sfmaterial] DLC CDB keyed materials: {}",
        index.material_count()
    );
    assert_eq!(index.material_count(), 500_385, "SFBGS007 CDB key count");
    // #5326 — `Some` alone is satisfied by ANY keyed object, textures or
    // not; the DLC CDB's `Components` <-> stream alignment is only proven
    // by the same five slots the base test pins.
    let mat = index
        .lookup("Materials\\Actors\\BipedA\\BipedA_Nightmare\\BipedA_Nightmare.mat")
        .expect("the DLC near-copy resolves the same base-corpus material");
    for (slot, suffix) in [
        (byroredux_sfmaterial::SLOT_COLOR, "bipeda_nightmare_color.dds"),
        (byroredux_sfmaterial::SLOT_NORMAL, "bipeda_nightmare_normal.dds"),
        (byroredux_sfmaterial::SLOT_ROUGHNESS, "bipeda_nightmare_rough.dds"),
        (byroredux_sfmaterial::SLOT_METALNESS, "bipeda_nightmare_metal.dds"),
        (byroredux_sfmaterial::SLOT_AMBIENT_OCCLUSION, "bipeda_nightmare_ao.dds"),
    ] {
        let got = mat
            .textures
            .iter()
            .find(|(s, _)| *s == slot)
            .map(|(_, p)| p.to_ascii_lowercase().replace('/', "\\"))
            .unwrap_or_default();
        assert!(got.ends_with(suffix), "DLC slot {slot} should carry *{suffix}, got {got:?}");
    }
}

/// #5502 — texture-set `Parent` inheritance on real data. A derived texture
/// set authors only the slots it changes; before the walk both materials
/// below resolved with no texture at all (2,049 of the 10,152 NIF-referenced
/// Meshes01 materials did; 217 still do after, 35 with no flat colour either
/// — the rest still resolve with no texture).
#[test]
#[ignore = "needs Starfield game data on disk"]
fn material_index_resolves_inherited_texture_slots() {
    let Some(data) = data_dir() else {
        return;
    };
    let ba2 = Ba2Archive::open(data.join("Starfield - Materials.ba2")).expect("open materials BA2");
    let bytes = ba2
        .extract("materials\\materialsbeta.cdb")
        .expect("extract cdb");
    let index = MaterialIndex::build(&bytes).expect("index the vanilla CDB");
    let slot = |mat: &byroredux_sfmaterial::CdbMaterial, want: u8| {
        mat.textures
            .iter()
            .find(|(s, _)| *s == want)
            .map(|(_, p)| p.to_ascii_lowercase().replace('/', "\\"))
            .unwrap_or_default()
    };

    let hair = index
        .lookup("Materials\\Actors\\Human\\Hair\\Flat_Top.mat")
        .expect("probe-verified material");
    assert!(
        slot(&hair, byroredux_sfmaterial::SLOT_COLOR).ends_with("afro_hair_shared_brown_color.dds"),
        "the colour map comes from the texture set's ancestors, got {:?}",
        slot(&hair, byroredux_sfmaterial::SLOT_COLOR)
    );

    let metal = index
        .lookup("Materials\\Architecture\\Catwalks\\MetalPaintedScuffed01.mat")
        .expect("probe-verified material");
    assert!(
        slot(&metal, byroredux_sfmaterial::SLOT_NORMAL).ends_with("paintscuffed01_normal.dds"),
        "the normal map is inherited, got {:?}",
        slot(&metal, byroredux_sfmaterial::SLOT_NORMAL)
    );
    assert!(
        !metal.flat_color_slots.is_empty(),
        "the authored flat-colour replacement still applies"
    );
}
