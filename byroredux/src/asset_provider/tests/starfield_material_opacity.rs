//! Starfield opacity source and cutout rules
//! (docs/prd/starfield-material-maps.md §5.1, §5.2; #7421).

use std::sync::Arc;

use byroredux_nif::import::{ImportedMaterial, ImportedTextureSource, OpacityHeld};
use byroredux_sfmaterial::test_support::{opacity_index, OpacityCase};

use super::super::constant_texture::constant_texture_path;
use super::super::*;
use super::imported_mesh_with_material_path;
use super::starfield_mat::minimal_cdb_bytes;

const MAP: &str = "textures\\smm\\widget_opacity.dds";

/// One case per way opacity can resolve. The same list drives the database
/// and the loose-file producers.
const CASES: &[OpacityCase<'static>] = &[
    case("map", Some(true), Some(0.5), None, &[(Some(MAP), None)]),
    // SMM-SRC-005 — the texture wins over the same layer's replacement.
    case("both", Some(true), Some(0.5), None, &[(Some(MAP), Some(0.25))]),
    case("constant", Some(true), Some(0.5), None, &[(None, Some(0.25))]),
    // SMM-OPA-005 — the named layer alone, texture and replacement.
    case(
        "named",
        Some(true),
        Some(0.5),
        Some("MATERIAL_LAYER_1"),
        &[(Some(MAP), None), (None, Some(1.0))],
    ),
    case(
        "unresolved",
        Some(true),
        Some(0.5),
        Some("MATERIAL_LAYER_3"),
        &[(Some(MAP), None)],
    ),
    case(
        "unsupplied",
        Some(true),
        Some(0.5),
        Some("MATERIAL_LAYER_1"),
        &[(Some(MAP), None), (None, None)],
    ),
    case("noflag", None, Some(0.5), None, &[(Some(MAP), None)]),
    case("flagfalse", Some(false), Some(0.5), None, &[(Some(MAP), None)]),
    case("zerothreshold", Some(true), Some(0.0), None, &[(Some(MAP), None)]),
    case("nothreshold", Some(true), None, None, &[(Some(MAP), None)]),
    case("nothingauthored", Some(true), Some(0.5), None, &[(None, None)]),
];

const fn case<'a>(
    name: &'a str,
    has_opacity: Option<bool>,
    threshold: Option<f32>,
    source_layer: Option<&'a str>,
    layers: &'a [(Option<&'a str>, Option<f32>)],
) -> OpacityCase<'a> {
    // `path` holds the case name; `material_path` builds the full path.
    OpacityCase {
        path: name,
        has_opacity,
        threshold,
        source_layer,
        layers,
    }
}

fn material_path(name: &str) -> String {
    format!("materials\\smm\\{name}.mat")
}

/// What the merge decided, in comparable form.
#[derive(Debug, PartialEq)]
struct Resolved {
    opacity: Option<String>,
    held: Option<OpacityHeld>,
    cutout: Option<(f32, u8)>,
}

fn resolved(material: &ImportedMaterial, pool: &byroredux_core::string::StringPool) -> Resolved {
    Resolved {
        opacity: material
            .textures
            .opacity
            .and_then(|path| pool.resolve(path))
            .map(str::to_owned),
        held: material.opacity_held,
        cutout: material
            .alpha_test
            .then_some((material.alpha_threshold, material.alpha_test_func)),
    }
}

fn expected(name: &str) -> Resolved {
    let source = |opacity: &str| Some(opacity.to_owned());
    let tested = Some((0.5, 4));
    let (opacity, held, cutout) = match name {
        "map" | "both" => (source(MAP), None, tested),
        // 0.25 of 255 rounds to 64.
        "constant" => (Some(constant_texture_path(64)), None, tested),
        "named" => (Some(constant_texture_path(255)), None, tested),
        "unresolved" => (None, Some(OpacityHeld::UnresolvedLayer), tested),
        "unsupplied" => (None, Some(OpacityHeld::UnsuppliedLayer), tested),
        "noflag" | "flagfalse" => (None, Some(OpacityHeld::NoHasOpacity), None),
        "zerothreshold" => (source(MAP), None, None),
        "nothreshold" => (source(MAP), None, Some((1.0 / 3.0, 4))),
        "nothingauthored" => (None, None, tested),
        other => panic!("no expectation for {other}"),
    };
    Resolved {
        opacity,
        held,
        cutout,
    }
}

/// A provider whose database holds every case, through the production lookup.
/// `source` keeps each test's entry in the process-wide index cache apart.
fn database_provider(source: &str) -> MaterialProvider {
    let paths: Vec<String> = CASES.iter().map(|c| material_path(c.path)).collect();
    let cases: Vec<OpacityCase> = CASES
        .iter()
        .zip(&paths)
        .map(|(c, path)| OpacityCase {
            path,
            has_opacity: c.has_opacity,
            threshold: c.threshold,
            source_layer: c.source_layer,
            layers: c.layers,
        })
        .collect();
    provider_over(opacity_index(&cases), source)
}

fn provider_over(index: byroredux_sfmaterial::MaterialIndex, source: &str) -> MaterialProvider {
    let inner = "materials\\materialsbeta.cdb";
    sf_cdb_index_cache_insert_for_test(&format!("{source}|{inner}"), Arc::new(index));
    let mut provider = MaterialProvider::new();
    assert!(
        crate::asset_provider::material::probe_starfield_cdb(&minimal_cdb_bytes(), "test")
            .is_some()
    );
    provider.register_starfield_cdb_probe(source.into(), inner.into());
    provider
}

fn merge(
    provider: &mut MaterialProvider,
    name: &str,
    nif_opacity: Option<&str>,
) -> (ImportedMaterial, byroredux_core::string::StringPool) {
    let mut pool = byroredux_core::string::StringPool::new();
    let mut mesh = imported_mesh_with_material_path(&mut pool, &material_path(name));
    mesh.material.textures.opacity = nif_opacity.map(|path| pool.intern(path));
    // Every NIF-named path resolves, so the dead-path repair (#4636) is idle.
    let _ = merge_external_material(&mut mesh.material, provider, &mut pool, &|_| true);
    (mesh.material, pool)
}

/// SMM-SRC-001 / SRC-003..007 — the four sources, decided by the merge.
#[test]
fn cdb_property_sources_are_decided_at_the_merge() {
    let mut provider = database_provider("smm-sources");
    for case in CASES {
        let (material, pool) = merge(&mut provider, case.path, None);
        assert_eq!(resolved(&material, &pool), expected(case.path), "{}", case.path);
        // A filled role is a `.mat` source; an empty one stays the default.
        let source = material.texture_sources.opacity;
        if material.textures.opacity.is_some() {
            assert_eq!(source, ImportedTextureSource::Mat, "{}", case.path);
        }
    }

    // Override (SMM-SRC-006): an authored NIF path beats a database Map and
    // a database Constant, and keeps its own provenance.
    for name in ["map", "constant"] {
        let (material, pool) = merge(&mut provider, name, Some("nif\\authored_opacity.dds"));
        assert_eq!(
            resolved(&material, &pool).opacity.as_deref(),
            Some("nif\\authored_opacity.dds")
        );
        assert_eq!(
            material.texture_sources.opacity,
            ImportedTextureSource::NifTextureSet
        );
    }

    // No later stage fills the role: the merge is its only writer.
    for (file, source) in [
        ("material_translate.rs", include_str!("../../material_translate.rs")),
        ("scene/mesh_entity.rs", include_str!("../../scene/mesh_entity.rs")),
        (
            "cell_loader/spawn/mesh_instance.rs",
            include_str!("../../cell_loader/spawn/mesh_instance.rs"),
        ),
        ("render/static_meshes.rs", include_str!("../../render/static_meshes.rs")),
    ] {
        let production = byroredux_core::source_scan::production_text(source);
        assert!(
            !production.contains("textures.opacity ="),
            "{file} assigns the opacity role after the merge"
        );
    }
}

/// A loose `.mat` file authoring `case`, in the installed files' layout.
fn loose_file(case: &OpacityCase) -> String {
    let component = |kind: &str, index: usize, data: String| {
        format!(r#"{{"Type":"BSMaterial::{kind}","Index":{index},"Data":{{{data}}}}}"#)
    };
    let mut alpha = Vec::new();
    if let Some(flag) = case.has_opacity {
        alpha.push(format!(r#""HasOpacity":"{flag}""#));
    }
    if let Some(threshold) = case.threshold {
        alpha.push(format!(r#""AlphaTestThreshold":"{threshold}""#));
    }
    if let Some(name) = case.source_layer {
        alpha.push(format!(r#""OpacitySourceLayer":"{name}""#));
    }
    let mut root = vec![component("AlphaSettingsComponent", 0, alpha.join(","))];
    let mut objects = Vec::new();
    for (number, (texture, replacement)) in case.layers.iter().enumerate() {
        root.push(component("LayerID", number, format!(r#""ID":"layer{number}""#)));
        let mut set = Vec::new();
        if let Some(texture) = texture {
            // The installed files prefix `Data\`, escaped for JSON.
            let file = format!("Data\\{texture}").replace('\\', "\\\\");
            set.push(component("MRTextureFile", 2, format!(r#""FileName":"{file}""#)));
        }
        if let Some(red) = replacement {
            let color = format!(
                r#""Enabled":"true","Color":{{"Type":"BSMaterial::Color","Data":{{"Value":{{"Type":"XMFLOAT4","Data":{{"x":"{red}","y":"0","z":"0","w":"1"}}}}}}}}"#
            );
            set.push(component("TextureReplacement", 2, color));
        }
        let link = |kind: &str, id: String| component(kind, 0, format!(r#""ID":"{id}""#));
        objects.push(format!(
            r#"{{"ID":"layer{number}","Components":[{}]}}"#,
            link("MaterialID", format!("material{number}"))
        ));
        objects.push(format!(
            r#"{{"ID":"material{number}","Components":[{}]}}"#,
            link("TextureSetID", format!("set{number}"))
        ));
        objects.push(format!(
            r#"{{"ID":"set{number}","Components":[{}]}}"#,
            set.join(",")
        ));
    }
    // A LOD material with alpha settings of its own hangs off the root in the
    // installed files. The walk must not take them.
    let lod = component(
        "AlphaSettingsComponent",
        0,
        r#""HasOpacity":"true","AlphaTestThreshold":"0.9","OpacitySourceLayer":"MATERIAL_LAYER_0""#
            .to_owned(),
    );
    objects.push(format!(r#"{{"ID":"lod","Components":[{lod}]}}"#));
    format!(
        r#"{{"Version":1,"Objects":[{{"Components":[{}]}},{}]}}"#,
        root.join(","),
        objects.join(",")
    )
}

/// SMM-SRC-002 — a loose material resolves the same source and value as a
/// database material authoring the same data. Both go through
/// `merge_external_material`: one over the database, one over loose files with
/// an empty database behind them.
#[test]
fn loose_and_database_materials_resolve_the_same_property_sources() {
    let mut database = database_provider("smm-parity");

    let loose_root = tempfile::tempdir().expect("temp dir");
    let folder = loose_root.path().join("materials").join("smm");
    std::fs::create_dir_all(&folder).expect("material folder");
    for case in CASES {
        std::fs::write(folder.join(format!("{}.mat", case.path)), loose_file(case))
            .expect("loose material");
    }
    let mut loose = provider_over(opacity_index(&[]), "smm-parity-loose");
    loose
        .archives
        .push(Archive::open_loose(loose_root.path()).expect("loose tier"));

    for case in CASES {
        let (from_database, database_pool) = merge(&mut database, case.path, None);
        let (from_file, file_pool) = merge(&mut loose, case.path, None);
        let want = expected(case.path);
        assert_eq!(resolved(&from_file, &file_pool), want, "{}: loose", case.path);
        assert_eq!(
            resolved(&from_database, &database_pool),
            want,
            "{}: database",
            case.path
        );
    }
}

/// SMM-SRC-002 — a loose file is a diff: the alpha settings it leaves
/// unauthored are its database parent's, and its own win. The installed
/// `portablegreenhouse02.mat` takes its cutout from `portablegreenhouse01`
/// this way.
#[test]
fn a_loose_diff_inherits_its_database_parents_alpha_settings() {
    let tested = Some((0.5, 4));
    let own_map = &[(Some(MAP), None)];
    let children: [(OpacityCase, &str, Resolved); 3] = [
        (
            case("child", None, None, None, own_map),
            "map",
            Resolved { opacity: Some(MAP.to_owned()), held: None, cutout: tested },
        ),
        (
            case("childoff", Some(false), None, None, own_map),
            "map",
            Resolved { opacity: None, held: Some(OpacityHeld::NoHasOpacity), cutout: None },
        ),
        // A parent the database does not hold gives nothing.
        (
            case("orphan", None, None, None, own_map),
            "absent_parent",
            Resolved { opacity: None, held: Some(OpacityHeld::NoHasOpacity), cutout: None },
        ),
    ];

    let loose_root = tempfile::tempdir().expect("temp dir");
    let folder = loose_root.path().join("materials").join("smm");
    std::fs::create_dir_all(&folder).expect("material folder");
    for (child, parent, _) in &children {
        let parented = format!(
            r#""Objects":[{{"Parent":"Data\\materials\\smm\\{parent}.mat","Components":["#
        );
        let json = loose_file(child).replacen(r#""Objects":[{"Components":["#, &parented, 1);
        assert!(json.contains(r#""Parent":"#), "the fixture root names its parent");
        std::fs::write(folder.join(format!("{}.mat", child.path)), json).expect("loose");
    }
    let mut provider = database_provider("smm-inherit");
    provider
        .archives
        .push(Archive::open_loose(loose_root.path()).expect("loose tier"));

    for (child, _, want) in children {
        let (material, pool) = merge(&mut provider, child.path, None);
        assert_eq!(resolved(&material, &pool), want, "{}", child.path);
    }
}

/// A7 — content with no Starfield material leaves the new lane at its default
/// (SMM-G-004).
#[test]
fn non_database_materials_leave_the_new_lanes_default() {
    // Paths that take every other merge route: BGSM, BGEM, an inline NIF
    // material, and a `.mat` with no database loaded.
    let mut provider = MaterialProvider::new();
    for path in [
        "materials\\fo4\\widget.bgsm",
        "materials\\fo4\\widget.bgem",
        "materials\\no_database\\widget.mat",
        "textures\\fnv\\inline.dds",
    ] {
        let mut pool = byroredux_core::string::StringPool::new();
        let mut mesh = imported_mesh_with_material_path(&mut pool, path);
        mesh.material.alpha_test = true;
        mesh.material.alpha_threshold = 0.5;
        mesh.material.alpha_test_func = 6;
        let _ = merge_external_material(&mut mesh.material, &mut provider, &mut pool, &|_| true);
        assert_eq!(mesh.material.textures.opacity, None, "{path}");
        assert_eq!(mesh.material.opacity_held, None, "{path}");
        assert_eq!(mesh.material.alpha_test_func, 6, "{path}: cutout untouched");
    }
    // The lane's default reaches the GPU record as zero.
    assert_eq!(
        byroredux_renderer::vulkan::material::GpuMaterial::default().opacity_map_index,
        0
    );
}

/// SMM-OBS-006 (A2) — over the installed models of the PRD §3.3 population,
/// every material that authors opacity data ends on every shape that names it
/// as Map, Constant or Override, or under a recorded rule. Prints the counts.
///
///   cargo test --release -p byroredux --bin byroredux \
///     starfield_material_property_reach -- --ignored --nocapture
#[test]
#[ignore = "needs Starfield game data on disk; parses every model"]
fn starfield_material_property_reach() {
    use std::collections::{BTreeMap, BTreeSet};

    use byroredux_plugin::esm::test_paths::{require_or_skip, starfield_data_dir};

    let data = starfield_data_dir();
    if !data.join("Starfield - Materials.ba2").is_file() {
        require_or_skip("the Starfield install");
        return;
    }
    let archive = |name: &str| data.join(name).to_string_lossy().into_owned();
    let model_archives: Vec<String> = [
        "Starfield - Meshes01.ba2",
        "Starfield - Meshes02.ba2",
        "Starfield - MeshesPatch.ba2",
    ]
    .into_iter()
    .map(archive)
    .collect();
    let mesh_args: Vec<String> = model_archives
        .iter()
        .flat_map(|path| ["--bsa".to_owned(), path.clone()])
        .collect();
    let meshes = build_texture_provider(&mesh_args);
    let mut materials = build_material_provider(&[
        "--materials-ba2".to_owned(),
        archive("Starfield - Materials.ba2"),
    ]);
    assert!(materials.has_starfield_cdb(), "no material database found");

    /// What one shape's opacity ended as.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum End {
        Map,
        Constant,
        Override,
        Held(OpacityHeld),
        NoRule,
    }
    // A database record, whatever spelling of its path a model uses.
    type Record = (u32, u32);
    // Record → (one spelling, the ends its shapes reached), for materials
    // that author opacity data.
    let mut ends: BTreeMap<Record, (String, BTreeSet<End>)> = BTreeMap::new();
    let mut resolved: BTreeSet<Record> = BTreeSet::new();
    let mut authors: BTreeMap<Record, bool> = BTreeMap::new();
    let (mut models, mut shapes) = (0usize, 0usize);

    for path in &model_archives {
        let ba2 = byroredux_bsa::Ba2Archive::open(path).expect("mesh archive");
        let entries: Vec<String> = ba2
            .list_files()
            .into_iter()
            .filter(|n| byroredux_nif::corpus::is_nif_entry(n))
            .map(str::to_owned)
            .collect();
        for entry in entries {
            let Ok(bytes) = ba2.extract(&entry) else {
                continue;
            };
            let Ok(scene) = byroredux_nif::parse_nif(&bytes) else {
                continue;
            };
            models += 1;
            let mut pool = byroredux_core::string::StringPool::new();
            let imported =
                byroredux_nif::import::import_nif_with_resolver(&scene, &mut pool, Some(&meshes));
            for mut mesh in imported {
                let Some(material_path) = mesh
                    .material
                    .material_path
                    .and_then(|p| pool.resolve(p))
                    .map(|p| p.to_ascii_lowercase().replace('/', "\\"))
                else {
                    continue;
                };
                let record = byroredux_sfmaterial::material_key(&material_path);
                let authored = *authors.entry(record).or_insert_with(|| {
                    let Some(found) = materials.lookup_cdb_material(&material_path) else {
                        return false;
                    };
                    resolved.insert(record);
                    let slot = byroredux_sfmaterial::SLOT_OPACITY;
                    found.textures.iter().any(|(s, path)| *s == slot && !path.is_empty())
                        || found.flat_color_slots.iter().any(|(s, _)| *s == slot)
                });
                if !authored {
                    continue;
                }
                shapes += 1;
                let _ = merge_external_material(
                    &mut mesh.material,
                    &mut materials,
                    &mut pool,
                    &|_| true,
                );
                let opacity = mesh.material.textures.opacity.and_then(|p| pool.resolve(p));
                let end = match (opacity, mesh.material.opacity_held) {
                    (Some(_), _)
                        if mesh.material.texture_sources.opacity != ImportedTextureSource::Mat =>
                    {
                        End::Override
                    }
                    (Some(path), _) if super::super::constant_texture::constant_texture_value(path)
                        .is_some() => End::Constant,
                    (Some(_), _) => End::Map,
                    (None, Some(held)) => End::Held(held),
                    (None, None) => End::NoRule,
                };
                ends.entry(record)
                    .or_insert_with(|| (material_path, BTreeSet::new()))
                    .1
                    .insert(end);
            }
        }
    }

    // A material reaches only if every shape that names it does.
    let reaches = |set: &BTreeSet<End>| {
        set.iter()
            .all(|end| matches!(end, End::Map | End::Constant | End::Override))
    };
    let mut by_source: BTreeMap<End, usize> = BTreeMap::new();
    let mut by_rule: BTreeMap<End, usize> = BTreeMap::new();
    let mut without_rule = Vec::new();
    for (path, set) in ends.values() {
        if reaches(set) {
            for end in set {
                *by_source.entry(*end).or_default() += 1;
            }
        } else {
            for end in set.iter().filter(|end| matches!(end, End::Held(_))) {
                *by_rule.entry(*end).or_default() += 1;
            }
            if set.contains(&End::NoRule) {
                without_rule.push(path.clone());
            }
        }
    }
    let reached = ends.values().filter(|(_, set)| reaches(set)).count();
    println!("models parsed: {models}; materials on imported shapes that resolve: {}",
        resolved.len());
    println!("opacity: {} materials author data ({shapes} shapes); {reached} reach", ends.len());
    for (end, count) in &by_source {
        println!("  reaches as {end:?}: {count}");
    }
    for (end, count) in &by_rule {
        if let End::Held(held) = end {
            println!("  held by {}: {count}", held.rule());
        }
    }
    println!("  no recorded rule: {}", without_rule.len());
    for path in without_rule.iter().take(20) {
        println!("    {path}");
    }
    assert!(!ends.is_empty(), "no authoring material was seen");
    assert!(without_rule.is_empty(), "materials with no source and no rule");
}
