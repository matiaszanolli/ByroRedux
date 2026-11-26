//! SMM-OBS-006 (A2): how far each delivered Starfield material-map property
//! reaches over the installed models (docs/prd/starfield-material-maps.md).

use std::collections::{BTreeMap, BTreeSet};

use byroredux_nif::import::{ImportedMaterial, ImportedTextureSource, OpacityHeld, PbrSource};
use byroredux_plugin::esm::test_paths::{require_or_skip, starfield_data_dir};
use byroredux_sfmaterial::{CdbMaterial, SLOT_METALNESS, SLOT_OPACITY, SLOT_ROUGHNESS};

use super::super::constant_texture::constant_texture_value;
use super::super::*;

/// What one shape's property ended as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum End {
    Map,
    Constant,
    Override,
    /// Held Absent by a recorded rule. Only opacity has any.
    Held(OpacityHeld),
    NoRule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Property {
    Opacity,
    Roughness,
    Metalness,
}

const PROPERTIES: [Property; 3] = [Property::Opacity, Property::Roughness, Property::Metalness];

impl Property {
    fn slot(self) -> u8 {
        match self {
            Self::Opacity => SLOT_OPACITY,
            Self::Roughness => SLOT_ROUGHNESS,
            Self::Metalness => SLOT_METALNESS,
        }
    }

    /// Whether any layer authors a texture or an enabled replacement. An empty
    /// path is no texture.
    fn is_authored(self, found: &CdbMaterial) -> bool {
        let slot = self.slot();
        found.textures.iter().any(|(s, path)| *s == slot && !path.is_empty())
            || found.flat_color_slots.iter().any(|(s, _)| *s == slot)
    }

    /// Where a merged shape's property ended.
    fn end(self, material: &ImportedMaterial, pool: &byroredux_core::string::StringPool) -> End {
        match self {
            Self::Opacity => {
                let opacity = material.textures.opacity.and_then(|p| pool.resolve(p));
                match (opacity, material.opacity_held) {
                    (Some(_), _)
                        if material.texture_sources.opacity != ImportedTextureSource::Mat =>
                    {
                        End::Override
                    }
                    (Some(path), _) if constant_texture_value(path).is_some() => End::Constant,
                    (Some(_), _) => End::Map,
                    (None, Some(held)) => End::Held(held),
                    (None, None) => End::NoRule,
                }
            }
            Self::Roughness => {
                scalar_end(material.roughness_source, material.textures.roughness.is_some())
            }
            Self::Metalness => {
                scalar_end(material.metalness_source, material.textures.metalness.is_some())
            }
        }
    }
}

fn scalar_end(source: Option<PbrSource>, bound: bool) -> End {
    match source {
        Some(PbrSource::Map) if bound => End::Map,
        Some(PbrSource::Constant(_)) => End::Constant,
        Some(PbrSource::Override) => End::Override,
        _ => End::NoRule,
    }
}

/// One property's records: record → (one spelling, the ends its shapes
/// reached), for materials that author it.
#[derive(Default)]
struct Reach {
    ends: BTreeMap<(u32, u32), (String, BTreeSet<End>)>,
    shapes: usize,
}

impl Reach {
    /// Print the counts and return the materials with no source and no rule.
    fn report(&self, property: Property) -> Vec<String> {
        // A material reaches only if every shape that names it does.
        let reaches = |set: &BTreeSet<End>| {
            set.iter()
                .all(|end| matches!(end, End::Map | End::Constant | End::Override))
        };
        let mut by_source: BTreeMap<End, usize> = BTreeMap::new();
        let mut by_rule: BTreeMap<End, usize> = BTreeMap::new();
        let mut without_rule = Vec::new();
        for (path, set) in self.ends.values() {
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
        let reached = self.ends.values().filter(|(_, set)| reaches(set)).count();
        println!(
            "{property:?}: {} materials author data ({} shapes); {reached} reach",
            self.ends.len(),
            self.shapes
        );
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
        without_rule
    }
}

/// SMM-OBS-006 (A2) — over the installed models of the PRD §3.3 population,
/// every material that authors a delivered property ends on every shape that
/// names it as Map, Constant or Override, or under a recorded rule. Prints the
/// counts.
///
///   cargo test --release -p byroredux --bin byroredux \
///     starfield_material_property_reach -- --ignored --nocapture
#[test]
#[ignore = "needs Starfield game data on disk; parses every model"]
fn starfield_material_property_reach() {
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

    // A database record, whatever spelling of its path a model uses.
    type Record = (u32, u32);
    let mut reach: [Reach; 3] = Default::default();
    let mut resolved: BTreeSet<Record> = BTreeSet::new();
    let mut authors: BTreeMap<Record, [bool; 3]> = BTreeMap::new();
    let mut models = 0usize;

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
                        return [false; 3];
                    };
                    resolved.insert(record);
                    PROPERTIES.map(|property| property.is_authored(&found))
                });
                if authored == [false; 3] {
                    continue;
                }
                let _ = merge_external_material(
                    &mut mesh.material,
                    &mut materials,
                    &mut pool,
                    &|_| true,
                );
                for (index, property) in PROPERTIES.into_iter().enumerate() {
                    if !authored[index] {
                        continue;
                    }
                    reach[index].shapes += 1;
                    reach[index]
                        .ends
                        .entry(record)
                        .or_insert_with(|| (material_path.clone(), BTreeSet::new()))
                        .1
                        .insert(property.end(&mesh.material, &pool));
                }
            }
        }
    }

    println!(
        "models parsed: {models}; materials on imported shapes that resolve: {}",
        resolved.len()
    );
    let mut missing = Vec::new();
    for (index, property) in PROPERTIES.into_iter().enumerate() {
        assert!(!reach[index].ends.is_empty(), "{property:?}: no authoring material was seen");
        missing.extend(reach[index].report(property));
    }
    assert!(missing.is_empty(), "materials with no source and no rule: {missing:?}");
}
