use super::*;

fn id(classified: Classified<'_>) -> &str {
    match classified {
        Classified::Mechanism(m) => m.id.as_str(),
        Classified::NotGameData => "not game data",
        Classified::Unclassified => "unclassified",
    }
}

const SMALL: &str = r#"
catalog_version = 9
games = ["fnv", "skyrim"]

[[mechanism]]
id = "A6"
name = "meshes"
disposition = "D1"
games = ["fnv", "skyrim"]
paths = ["meshes/**/*.nif"]

[[mechanism]]
id = "A9"
name = "facegen"
disposition = "D1"
games = ["skyrim"]
paths = ["meshes/**/facegendata/**"]

[[mechanism]]
id = "S8"
name = "papyrus ini"
disposition = "D2"
per_game = { skyrim = "D3" }
games = ["fnv", "skyrim"]
ini_keys = ["papyrus/*"]

[not_game_data]
paths = ["*.txt", "fomod/**"]
"#;

#[test]
fn the_most_specific_pattern_wins_and_a_mechanism_applies_only_to_its_games() {
    let catalog = ModCatalog::parse(SMALL).unwrap();
    assert_eq!(catalog.version, 9);
    assert_eq!(id(catalog.classify_path("skyrim", "Meshes\\Armor\\Iron.nif")), "A6");
    // `meshes/**/facegendata/**` has more literal characters than `meshes/**/*.nif`.
    let head = "meshes/actors/character/facegendata/facegeom/skyrim.esm/00013BA3.NIF";
    assert_eq!(id(catalog.classify_path("skyrim", head)), "A9");
    // A9 is not a New Vegas mechanism, so the mesh pattern is the one that applies there.
    assert_eq!(id(catalog.classify_path("fnv", head)), "A6");
    // `**` matches zero segments.
    assert_eq!(id(catalog.classify_path("fnv", "meshes/x.nif")), "A6");
    assert_eq!(id(catalog.classify_path("fnv", "meshes/x.dds")), "unclassified");
}

#[test]
fn documentation_is_not_game_data_and_a_star_stays_within_one_segment() {
    let catalog = ModCatalog::parse(SMALL).unwrap();
    assert_eq!(id(catalog.classify_path("fnv", "Readme.txt")), "not game data");
    assert_eq!(id(catalog.classify_path("fnv", "fomod/ModuleConfig.xml")), "not game data");
    assert_eq!(id(catalog.classify_path("fnv", "docs/readme.txt")), "unclassified", "`*.txt` is the root level only");
}

#[test]
fn ini_keys_classify_by_section_and_dispositions_follow_per_game() {
    let catalog = ModCatalog::parse(SMALL).unwrap();
    let keys = ini_keys("; comment\n[Papyrus]\nfPostLoadUpdateTimeMS=500.0\n\n[Display]\n iSize W = 1920\n");
    assert_eq!(keys, vec!["papyrus/fpostloadupdatetimems", "display/isize w"]);
    let Classified::Mechanism(s8) = catalog.classify_ini_key("skyrim", &keys[0]) else { panic!("S8") };
    assert_eq!((s8.id.as_str(), s8.disposition_for("skyrim"), s8.disposition_for("fnv")), ("S8", "D3", "D2"));
    assert_eq!(id(catalog.classify_ini_key("skyrim", &keys[1])), "unclassified");
}

/// The checked-in catalog classifies the files the Wave 1 games' mods ship.
#[test]
fn the_embedded_catalog_classifies_common_mod_files() {
    let catalog = ModCatalog::embedded();
    for (game, path, want) in [
        ("fnv", "YUP - Base Game + All DLC.esm", "R1"),
        ("fnv", "NVSE/Plugins/nvac.dll", "N2"),
        ("fnv", "NVSE/Plugins/nvse_config.ini", "T8"),
        ("fnv", "Textures/Clutter/Food/Apple.dds", "A5"),
        ("fnv", "Menus/Prefabs/duinv.xml", "U3"),
        ("skyrim", "Scripts/SKI_ConfigBase.pex", "S1"),
        ("skyrim", "SKSE/Plugins/PapyrusUtil.dll", "S5"),
        ("skyrim", "meshes/actors/character/behaviors/0_master.hkx", "N6"),
        ("skyrim", "Interface/Translations/SkyUI_ENGLISH.txt", "U4"),
        ("skyrim", "SKSE/Plugins/SomethingUnknown.dll", "unclassified"),
        ("skyrim", "fomod/info.xml", "not game data"),
    ] {
        assert_eq!(id(catalog.classify_path(game, path)), want, "{game} {path}");
    }
    assert_eq!(id(catalog.classify_game_dir("skyrim", "skse64_loader.exe")), "N1.binary");
    assert_eq!(id(catalog.classify_game_dir("skyrim", "d3d11.dll")), "N5");
    assert_eq!(id(catalog.classify_game_dir("skyrim", "SkyrimSE.exe")), "unclassified");
    assert_eq!(id(catalog.classify_ini_key("skyrim", "general/ugridstoload")), "T5");
}
