//! Starfield opacity source and cutout rules
//! (docs/prd/starfield-material-maps.md §5.1, §5.2; #7421).

use std::sync::Arc;

use byroredux_nif::import::{ImportedMaterial, ImportedTextureSource, OpacityHeld};
use byroredux_sfmaterial::test_support::{opacity_index, OpacityCase, SlotEntry};
use byroredux_sfmaterial::SLOT_OPACITY;

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

pub(super) fn material_path(name: &str) -> String {
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

pub(super) fn provider_over(index: byroredux_sfmaterial::MaterialIndex, source: &str) -> MaterialProvider {
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

pub(super) fn merge(
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
    // Roughness and metalness have their own cases (#7422).
    super::starfield_material_pbr::assert_database_sources();

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
    let root = vec![loose_component("AlphaSettingsComponent", 0, alpha.join(","))];
    let layers: Vec<Vec<SlotEntry>> = case
        .layers
        .iter()
        .map(|&(texture, red)| vec![(SLOT_OPACITY, texture, red)])
        .collect();
    // A LOD material with alpha settings of its own hangs off the root in the
    // installed files. The walk must not take them.
    let lod = loose_component(
        "AlphaSettingsComponent",
        0,
        r#""HasOpacity":"true","AlphaTestThreshold":"0.9","OpacitySourceLayer":"MATERIAL_LAYER_0""#
            .to_owned(),
    );
    let lod = format!(r#"{{"ID":"lod","Components":[{lod}]}}"#);
    loose_document(root, None, &layers, vec![lod])
}

pub(super) fn loose_component(kind: &str, index: usize, data: String) -> String {
    format!(r#"{{"Type":"BSMaterial::{kind}","Index":{index},"Data":{{{data}}}}}"#)
}

/// What one texture set authors, as loose components.
fn loose_slot_components(entries: &[SlotEntry]) -> Vec<String> {
    let mut set = Vec::new();
    for &(slot, texture, replacement) in entries {
        let slot = usize::from(slot);
        if let Some(texture) = texture {
            // The installed files prefix `Data\`, escaped for JSON.
            let file = format!("Data\\{texture}").replace('\\', "\\\\");
            set.push(loose_component("MRTextureFile", slot, format!(r#""FileName":"{file}""#)));
        }
        if let Some(red) = replacement {
            let color = format!(
                r#""Enabled":"true","Color":{{"Type":"BSMaterial::Color","Data":{{"Value":{{"Type":"XMFLOAT4","Data":{{"x":"{red}","y":"0","z":"0","w":"1"}}}}}}}}"#
            );
            set.push(loose_component("TextureReplacement", slot, color));
        }
    }
    set
}

/// The installed files' object graph: `root_components` on the root, an
/// optional root-named texture set, and numbered layers, each reaching its
/// texture set through a material.
pub(super) fn loose_document(
    mut root_components: Vec<String>,
    root_set: Option<&[SlotEntry]>,
    layers: &[Vec<SlotEntry>],
    extra_objects: Vec<String>,
) -> String {
    let link = |kind: &str, id: String| loose_component(kind, 0, format!(r#""ID":"{id}""#));
    let object = |id: String, components: Vec<String>| {
        format!(r#"{{"ID":"{id}","Components":[{}]}}"#, components.join(","))
    };
    let mut objects = Vec::new();
    if let Some(entries) = root_set {
        root_components.push(link("TextureSetID", "rootset".to_owned()));
        objects.push(object("rootset".to_owned(), loose_slot_components(entries)));
    }
    for (number, entries) in layers.iter().enumerate() {
        root_components.push(loose_component(
            "LayerID",
            number,
            format!(r#""ID":"layer{number}""#),
        ));
        objects.push(object(
            format!("layer{number}"),
            vec![link("MaterialID", format!("material{number}"))],
        ));
        objects.push(object(
            format!("material{number}"),
            vec![link("TextureSetID", format!("set{number}"))],
        ));
        objects.push(object(format!("set{number}"), loose_slot_components(entries)));
    }
    objects.extend(extra_objects);
    format!(
        r#"{{"Version":1,"Objects":[{{"Components":[{}]}},{}]}}"#,
        root_components.join(","),
        objects.join(",")
    )
}

/// A loose-file tier holding `files` (name, contents) as `materials\smm\`.
pub(super) fn loose_materials(
    files: impl Iterator<Item = (String, String)>,
) -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("temp dir");
    let folder = root.path().join("materials").join("smm");
    std::fs::create_dir_all(&folder).expect("material folder");
    for (name, contents) in files {
        std::fs::write(folder.join(format!("{name}.mat")), contents).expect("loose material");
    }
    root
}

/// SMM-SRC-002 — a loose material resolves the same source and value as a
/// database material authoring the same data. Both go through
/// `merge_external_material`: one over the database, one over loose files with
/// an empty database behind them.
#[test]
fn loose_and_database_materials_resolve_the_same_property_sources() {
    // Roughness and metalness have their own cases (#7422).
    super::starfield_material_pbr::assert_loose_parity();

    let mut database = database_provider("smm-parity");

    let loose_root =
        loose_materials(CASES.iter().map(|case| (case.path.to_owned(), loose_file(case))));
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

/// A7 — content with no Starfield material leaves the new lanes at their default
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
        assert_eq!(mesh.material.textures.roughness, None, "{path}");
        assert_eq!(mesh.material.textures.metalness, None, "{path}");
        assert_eq!(mesh.material.roughness_source, None, "{path}");
        assert_eq!(mesh.material.metalness_source, None, "{path}");
        assert_eq!(mesh.material.opacity_held, None, "{path}");
        assert_eq!(mesh.material.alpha_test_func, 6, "{path}: cutout untouched");
    }
    // The lanes' defaults reach the GPU record as zero.
    let record = byroredux_renderer::vulkan::material::GpuMaterial::default();
    assert_eq!(
        [record.opacity_map_index, record.roughness_map_index, record.metalness_map_index],
        [0; 3]
    );
}
