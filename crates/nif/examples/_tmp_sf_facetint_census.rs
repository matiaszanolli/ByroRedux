//! Throwaway (#3796 census, 2026-08-31): does any full (non-stub) Starfield
//! BSLightingShaderProperty carry a populated BSShaderTextureSet, and if so
//! what raw shader_type values occur? Settles the contradiction between
//! slot_role.rs's module header ("Starfield BSGeometry materials deliberately
//! do not enter this table") and canonical_shader_type's doc ("a Starfield
//! FaceTint (3) reached the slot table as Skyrim Parallax").
use byroredux_bsa::Ba2Archive;
use byroredux_nif::blocks::shader::{BSLightingShaderProperty, BSShaderTextureSet};

const STARFIELD_BSVER_MIN: u32 = 172;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut full_count = 0u64;
    let mut full_with_resolved_nonempty_texset = 0u64;
    let mut shader_type_histogram: std::collections::BTreeMap<u32, u64> = Default::default();
    let mut samples: Vec<String> = Vec::new();

    for path in &args {
        let Ok(archive) = Ba2Archive::open(path) else {
            eprintln!("skip {path}");
            continue;
        };
        let names: Vec<String> = archive
            .list_files()
            .into_iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
            .map(|s| s.to_string())
            .collect();
        eprintln!("{path}: {} nifs", names.len());
        for name in names {
            let Ok(bytes) = archive.extract(&name) else {
                continue;
            };
            let Ok(scene) = byroredux_nif::parse_nif(&bytes) else {
                continue;
            };
            if scene.bsver < STARFIELD_BSVER_MIN {
                continue;
            }
            for i in 0..scene.blocks.len() {
                let Some(s) = scene.get_as::<BSLightingShaderProperty>(i) else {
                    continue;
                };
                if s.material_reference {
                    continue; // stub — routes via BGSM/CDB, not this table.
                }
                full_count += 1;
                let Some(ti) = s.texture_set_ref.index() else {
                    continue;
                };
                let Some(ts) = scene.get_as::<BSShaderTextureSet>(ti) else {
                    continue;
                };
                if ts.textures.iter().any(|t| !t.is_empty()) {
                    full_with_resolved_nonempty_texset += 1;
                    *shader_type_histogram.entry(s.shader_type).or_insert(0) += 1;
                    if samples.len() < 10 {
                        samples.push(format!(
                            "{name}: shader_type={} textures={:?}",
                            s.shader_type, ts.textures
                        ));
                    }
                }
            }
        }
    }

    println!("full (non-stub) BSLightingShaderProperty @ bsver>=172: {full_count}");
    println!(
        "  of those, with a resolved BSShaderTextureSet carrying any non-empty slot: {full_with_resolved_nonempty_texset}"
    );
    println!("  raw shader_type histogram over that set: {shader_type_histogram:?}");
    for s in &samples {
        println!("  sample {s}");
    }
}
