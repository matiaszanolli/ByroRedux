use super::*;

#[test]
fn the_catalog_game_comes_from_the_header_and_the_base_master() {
    assert_eq!(catalog_game(GameKind::Fallout3NV, Path::new("/d/FalloutNV.esm")), Some("fnv"));
    assert_eq!(catalog_game(GameKind::Fallout3NV, Path::new("/d/Fallout3.esm")), Some("fo3"));
    assert_eq!(catalog_game(GameKind::Skyrim, Path::new("/d/Skyrim.esm")), Some("skyrim"));
    assert_eq!(catalog_game(GameKind::Fallout76, Path::new("/d/SeventySix.esm")), None);
}

fn root(identity: &str, unmanaged: bool) -> ModRoot {
    ModRoot {
        identity: identity.to_owned(),
        path: PathBuf::from(format!("/mods/{identity}")),
        unmanaged,
        plugins: Vec::new(),
        archives: Vec::new(),
        profile_plugins: Vec::new(),
        disabled: false,
    }
}

/// LMC-AST-010: a later root's loose file wins over an earlier root's, any loose file over any archive, and a later
/// archive over an earlier one.
#[test]
fn an_asset_conflict_names_the_source_that_wins() {
    let roots = vec![root("unmanaged", true), root("moda", false), root("modb", false)];
    let mut sources: BTreeMap<String, Sources> = BTreeMap::new();
    sources.insert("textures/a.dds".into(), vec![(1, None), (2, None)]);
    sources.insert("textures/b.dds".into(), vec![(2, Some(("modb.bsa".into(), 1))), (1, None)]);
    sources.insert("textures/c.dds".into(), vec![(2, Some(("modb.bsa".into(), 1))), (1, Some(("moda.bsa".into(), 0)))]);
    sources.insert("textures/d.dds".into(), vec![(1, None)]);
    let lines = conflict_lines(&sources, &roots);
    assert_eq!(
        lines,
        vec![
            "textures/a.dds: modb (loose) over moda (loose)",
            "textures/b.dds: moda (loose) over modb (modb.bsa)",
            "textures/c.dds: modb (modb.bsa) over moda (moda.bsa)",
        ]
    );
}

/// A generated New Vegas mod root: every file is classified by the catalog, an unknown DLL and an unknown INI key are
/// unclassified, documentation is set apart, and nothing is loaded.
#[test]
fn a_mod_root_is_classified_file_by_file() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("mymod");
    for (path, bytes) in [
        ("MyMod.esp", &b"not parsed in this test"[..]),
        ("Inactive.esp", b""),
        ("textures/clutter/apple.dds", b""),
        ("meshes/clutter/apple.nif", b""),
        ("NVSE/Plugins/nvac.dll", b""),
        ("NVSE/Plugins/mystery.dll", b""),
        ("NVSE/Plugins/mymod.ini", b""),
        ("Readme.txt", b""),
        ("bodyslide/project.osp", b""),
    ] {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    let mut mymod = root("mymod", false);
    mymod.path = dir.clone();
    mymod.plugins = vec!["Inactive.esp".into(), "MyMod.esp".into()];
    mymod.profile_plugins = vec![(dir.join("MyMod.esp").to_string_lossy().into_owned(), false)];

    let mut scan = RootScan::default();
    let mut sources = BTreeMap::new();
    scan_root(&mymod, 1, "fnv", ModCatalog::embedded(), &HashMap::new(), &mut scan, &mut sources);

    let counts: BTreeMap<&str, usize> = scan.mechanisms.iter().map(|(id, t)| (id.as_str(), t.count)).collect();
    assert_eq!(counts, BTreeMap::from([("A5", 1), ("A6", 1), ("N2", 1), ("R1", 2), ("T8", 1)]));
    assert_eq!(scan.unclassified, vec!["NVSE/Plugins/mystery.dll", "bodyslide/project.osp"]);
    assert_eq!(scan.not_game_data, 1);
    assert!(scan.notes.contains(&"Inactive.esp: not in the profile".to_owned()), "{:?}", scan.notes);
    assert!(scan.notes.contains(&"MyMod.esp: disabled by activation".to_owned()), "{:?}", scan.notes);
    // The loose assets are the conflict section's sources; DLLs and INIs are not assets.
    assert_eq!(sources.keys().collect::<Vec<_>>(), vec!["meshes/clutter/apple.nif", "textures/clutter/apple.dds"]);
}
