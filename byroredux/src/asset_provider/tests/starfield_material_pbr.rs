//! Starfield roughness and metalness sources
//! (docs/prd/starfield-material-maps.md §5.1, §5.3; #7422).
//!
//! The two PRD-named source tests live with the opacity cases
//! (`starfield_material_opacity`); each also runs the half here.

use byroredux_core::ecs::components::material::PbrMaterial;
use byroredux_nif::import::{ImportedMaterial, ImportedTextureSource, PbrSource};
use byroredux_sfmaterial::test_support::{layered_index, LayeredCase, SlotEntry};
use byroredux_sfmaterial::{SLOT_COLOR, SLOT_METALNESS, SLOT_ROUGHNESS};

use super::super::*;
use super::imported_mesh_with_material_path;
use super::starfield_material_opacity::{
    loose_document, loose_materials, material_path, merge, provider_over,
};

const ROUGH: &str = "textures\\smm\\widget_rough.dds";
const METAL: &str = "textures\\smm\\widget_metal.dds";

/// One material per way the two properties can resolve. The same list drives
/// the database and the loose-file producers.
struct Case {
    name: &'static str,
    root_set: &'static [SlotEntry<'static>],
    layers: &'static [&'static [SlotEntry<'static>]],
    roughness: PbrSource,
    metalness: PbrSource,
}

const fn case(
    name: &'static str,
    root_set: &'static [SlotEntry<'static>],
    layers: &'static [&'static [SlotEntry<'static>]],
    roughness: PbrSource,
    metalness: PbrSource,
) -> Case {
    Case {
        name,
        root_set,
        layers,
        roughness,
        metalness,
    }
}

use PbrSource::{Absent, Constant, Map};

const CASES: &[Case] = &[
    case(
        "pbrmap",
        &[],
        &[&[(SLOT_ROUGHNESS, Some(ROUGH), None), (SLOT_METALNESS, Some(METAL), None)]],
        Map,
        Map,
    ),
    case(
        "pbrconstant",
        &[],
        &[&[(SLOT_ROUGHNESS, None, Some(0.25)), (SLOT_METALNESS, None, Some(1.0))]],
        Constant(0.25),
        Constant(1.0),
    ),
    // SMM-SRC-005 — the texture wins over the same layer's replacement.
    case(
        "pbrboth",
        &[],
        &[&[(SLOT_ROUGHNESS, Some(ROUGH), Some(0.25)), (SLOT_METALNESS, Some(METAL), Some(0.5))]],
        Map,
        Map,
    ),
    // SMM-SRC-010 — each slot from the first layer supplying either half; the
    // two properties may come from different layers.
    case(
        "pbrhalves",
        &[],
        &[
            &[(SLOT_ROUGHNESS, None, Some(0.25))],
            &[(SLOT_ROUGHNESS, Some(ROUGH), None), (SLOT_METALNESS, Some(METAL), None)],
        ],
        Constant(0.25),
        Map,
    ),
    // The root-named set comes first.
    case(
        "pbrroot",
        &[(SLOT_ROUGHNESS, None, Some(0.75))],
        &[&[(SLOT_ROUGHNESS, Some(ROUGH), None)]],
        Constant(0.75),
        Absent,
    ),
    case("pbrabsent", &[], &[&[(SLOT_COLOR, None, Some(0.5))]], Absent, Absent),
];

fn database_provider(source: &str) -> MaterialProvider {
    let paths: Vec<String> = CASES.iter().map(|c| material_path(c.name)).collect();
    let cases: Vec<LayeredCase> = CASES
        .iter()
        .zip(&paths)
        .map(|(c, path)| LayeredCase {
            path,
            root_set: c.root_set,
            layers: c.layers,
        })
        .collect();
    provider_over(layered_index(&cases), source)
}

/// What the merge decided for both properties, in comparable form.
#[derive(Debug, PartialEq)]
struct Resolved {
    sources: [Option<PbrSource>; 2],
    scalars: [Option<f32>; 2],
    roles: [Option<String>; 2],
}

fn resolved(material: &ImportedMaterial, pool: &byroredux_core::string::StringPool) -> Resolved {
    let role = |path: Option<byroredux_core::string::FixedString>| {
        path.and_then(|path| pool.resolve(path)).map(str::to_owned)
    };
    Resolved {
        sources: [material.roughness_source, material.metalness_source],
        scalars: [material.roughness_override, material.metalness_override],
        roles: [role(material.textures.roughness), role(material.textures.metalness)],
    }
}

/// A Map and an Absent property both leave the neutral scalar (SMM-SRC-007,
/// SMM-SRC-011); a Constant sets it (SMM-PBR-003).
fn expected(case: &Case) -> Resolved {
    let neutral = PbrMaterial::NO_SIGNAL_NEUTRAL;
    let scalar = |source, neutral| match source {
        Constant(value) => Some(value),
        _ => Some(neutral),
    };
    let role = |source, path: &str| (source == Map).then(|| path.to_owned());
    Resolved {
        sources: [Some(case.roughness), Some(case.metalness)],
        scalars: [
            scalar(case.roughness, neutral.roughness),
            scalar(case.metalness, neutral.metalness),
        ],
        roles: [role(case.roughness, ROUGH), role(case.metalness, METAL)],
    }
}

/// SMM-SRC-001 / SRC-003..007 / SRC-010 for roughness and metalness.
pub(super) fn assert_database_sources() {
    let mut provider = database_provider("smm-pbr-sources");
    for case in CASES {
        let (material, pool) = merge(&mut provider, case.name, None);
        assert_eq!(resolved(&material, &pool), expected(case), "{}", case.name);
        for (role, source) in [
            (material.textures.roughness, material.texture_sources.roughness),
            (material.textures.metalness, material.texture_sources.metalness),
        ] {
            if role.is_some() {
                assert_eq!(source, ImportedTextureSource::Mat, "{}", case.name);
            }
        }
    }

    // The merge is the only writer of the two roles' source decision: a
    // material merged again keeps it, whatever the first pass stamped.
    let mut pool = byroredux_core::string::StringPool::new();
    let mut mesh = imported_mesh_with_material_path(&mut pool, &material_path("pbrconstant"));
    for pass in 0..2 {
        let _ = merge_external_material(&mut mesh.material, &mut provider, &mut pool, &|_| true);
        assert_eq!(mesh.material.roughness_source, Some(Constant(0.25)), "pass {pass}");
    }

    // SMM-SRC-006 — an authored scalar beats the database's Constant and Map,
    // and no map is bound under it.
    for name in ["pbrconstant", "pbrmap"] {
        let (material, _) = merge_with(&mut provider, name, |material, _| {
            material.roughness_override = Some(0.3);
            material.metalness_override = Some(0.6);
        });
        assert_eq!(material.roughness_source, Some(PbrSource::Override), "{name}");
        assert_eq!(material.metalness_source, Some(PbrSource::Override), "{name}");
        assert_eq!(material.roughness_override, Some(0.3), "{name}");
        assert_eq!(material.metalness_override, Some(0.6), "{name}");
        assert_eq!(material.textures.roughness, None, "{name}");
        assert_eq!(material.textures.metalness, None, "{name}");
    }
    // A texture something else put in the role keeps it.
    let (material, pool) = merge_with(&mut provider, "pbrmap", |material, pool| {
        material.textures.roughness = Some(pool.intern("nif\\authored_rough.dds"));
    });
    assert_eq!(material.roughness_source, Some(PbrSource::Override));
    assert_eq!(
        resolved(&material, &pool).roles[0].as_deref(),
        Some("nif\\authored_rough.dds")
    );
    assert_eq!(material.texture_sources.roughness, ImportedTextureSource::NifTextureSet);

    // The NIF classifier's guess is not an authored value: the database
    // replaces it, and an Absent property keeps it (today's behaviour).
    let guess = |material: &mut ImportedMaterial, _: &mut byroredux_core::string::StringPool| {
        material.pbr_classified_at_import = true;
        material.roughness_override = Some(0.3);
        material.metalness_override = Some(0.9);
    };
    let (material, _) = merge_with(&mut provider, "pbrroot", guess);
    assert_eq!(material.roughness_source, Some(Constant(0.75)));
    assert_eq!(material.roughness_override, Some(0.75));
    assert_eq!(material.metalness_source, Some(Absent));
    assert_eq!(material.metalness_override, Some(0.9));
    let (material, _) = merge_with(&mut provider, "pbrmap", guess);
    assert_eq!(material.roughness_source, Some(Map));
    // SMM-SRC-011 — the neutral, not the guess, shades if the map cannot load.
    assert_eq!(
        material.roughness_override,
        Some(PbrMaterial::NO_SIGNAL_NEUTRAL.roughness)
    );

    // No later stage assigns either role.
    for (file, source) in [
        ("material_translate.rs", include_str!("../../material_translate.rs")),
        ("scene/mesh_entity.rs", include_str!("../../scene/mesh_entity.rs")),
        ("render/static_meshes.rs", include_str!("../../render/static_meshes.rs")),
    ] {
        let production = byroredux_core::source_scan::production_text(source);
        for assignment in ["textures.roughness =", "textures.metalness ="] {
            assert!(!production.contains(assignment), "{file}: {assignment}");
        }
    }
}

/// [`merge`] with the imported material prepared first.
fn merge_with(
    provider: &mut MaterialProvider,
    name: &str,
    prepare: impl FnOnce(&mut ImportedMaterial, &mut byroredux_core::string::StringPool),
) -> (ImportedMaterial, byroredux_core::string::StringPool) {
    let mut pool = byroredux_core::string::StringPool::new();
    let mut mesh = imported_mesh_with_material_path(&mut pool, &material_path(name));
    prepare(&mut mesh.material, &mut pool);
    let _ = merge_external_material(&mut mesh.material, provider, &mut pool, &|_| true);
    (mesh.material, pool)
}

/// SMM-SRC-002 for roughness and metalness: loose files and the database
/// resolve every case the same way.
pub(super) fn assert_loose_parity() {
    let mut database = database_provider("smm-pbr-parity");
    let loose_root = loose_materials(CASES.iter().map(|case| {
        let layers: Vec<Vec<SlotEntry>> = case.layers.iter().map(|set| set.to_vec()).collect();
        let root_set = (!case.root_set.is_empty()).then_some(case.root_set);
        (case.name.to_owned(), loose_document(Vec::new(), root_set, &layers, Vec::new()))
    }));
    let mut loose = provider_over(layered_index(&[]), "smm-pbr-parity-loose");
    loose
        .archives
        .push(Archive::open_loose(loose_root.path()).expect("loose tier"));

    for case in CASES {
        let (from_database, database_pool) = merge(&mut database, case.name, None);
        let (from_file, file_pool) = merge(&mut loose, case.name, None);
        let want = expected(case);
        assert_eq!(resolved(&from_file, &file_pool), want, "{}: loose", case.name);
        assert_eq!(resolved(&from_database, &database_pool), want, "{}: database", case.name);
    }
}
