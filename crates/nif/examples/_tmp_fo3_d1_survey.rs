//! TEMP scratch: FO3 dimension-1 (inline shader / material) survey.
use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::shader::{
    BSShaderNoLightingProperty, BSShaderPPLightingProperty, SkyShaderProperty,
    TallGrassShaderProperty, TileShaderProperty, WaterShaderProperty,
};
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;

fn main() {
    let path = std::env::args().nth(1).expect("usage: <bsa>");
    let arc = BsaArchive::open(&path).expect("open bsa");
    let files: Vec<String> = arc
        .list_files()
        .into_iter()
        .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
        .map(|s| s.to_string())
        .collect();
    eprintln!("{} nif entries", files.len());

    let mut bsver_hist: BTreeMap<u32, usize> = BTreeMap::new();
    let mut pp = 0usize;
    let mut nl = 0usize;
    let mut tile = 0usize;
    let mut sky = 0usize;
    let mut grass = 0usize;
    let mut water = 0usize;
    let mut f1_bits = [0usize; 32];
    let mut f2_bits = [0usize; 32];
    let mut eye_only = 0usize; // bit17 set, bits 7 & 21 clear
    let mut envmap_scale_hist: BTreeMap<String, usize> = BTreeMap::new();
    let mut shader_type_hist: BTreeMap<u32, usize> = BTreeMap::new();
    let mut nl_flags2_decal_only = 0usize;
    let mut parsed = 0usize;

    // Imported-material tallies
    let mut kind_hist: BTreeMap<u32, usize> = BTreeMap::new();
    let mut emsrc_hist: BTreeMap<String, usize> = BTreeMap::new();
    let mut pbr_true = 0usize;
    let mut two_sided = 0usize;
    let mut decal = 0usize;
    let mut meshes = 0usize;

    for name in &files {
        let Ok(bytes) = arc.extract(name) else {
            continue;
        };
        let bsver = byroredux_nif::header::NifHeader::parse(&bytes)
            .map(|(h, _)| h.user_version_2)
            .unwrap_or(0);
        let Ok(scene) = parse_nif(&bytes) else {
            continue;
        };
        parsed += 1;
        *bsver_hist.entry(bsver).or_default() += 1;
        for block in scene.blocks.iter() {
            let a = block.as_any();
            if let Some(s) = a.downcast_ref::<BSShaderPPLightingProperty>() {
                pp += 1;
                let f1 = s.shader_flags_1();
                let f2 = s.shader_flags_2();
                for b in 0..32 {
                    if f1 & (1 << b) != 0 {
                        f1_bits[b] += 1;
                    }
                    if f2 & (1 << b) != 0 {
                        f2_bits[b] += 1;
                    }
                }
                if f1 & (1 << 17) != 0 && f1 & ((1 << 7) | (1 << 21)) == 0 {
                    eye_only += 1;
                }
                *shader_type_hist.entry(s.shader.shader_type).or_default() += 1;
                *envmap_scale_hist
                    .entry(format!("{:.2}", s.shader.env_map_scale))
                    .or_default() += 1;
            }
            if let Some(s) = a.downcast_ref::<BSShaderNoLightingProperty>() {
                nl += 1;
                let f1 = s.shader_flags_1();
                let f2 = s.shader_flags_2();
                if f1 & ((1 << 26) | (1 << 27)) == 0 && f2 & (1 << 21) != 0 {
                    nl_flags2_decal_only += 1;
                }
                *shader_type_hist
                    .entry(1000 + s.shader.shader_type)
                    .or_default() += 1;
            }
            if a.downcast_ref::<TileShaderProperty>().is_some() {
                tile += 1;
            }
            if a.downcast_ref::<SkyShaderProperty>().is_some() {
                sky += 1;
            }
            if a.downcast_ref::<TallGrassShaderProperty>().is_some() {
                grass += 1;
            }
            if a.downcast_ref::<WaterShaderProperty>().is_some() {
                water += 1;
            }
        }
        let mut pool = byroredux_core::string::StringPool::new();
        for m in byroredux_nif::import::import_nif(&scene, &mut pool) {
            meshes += 1;
            *kind_hist.entry(m.material.material_kind).or_default() += 1;
            *emsrc_hist
                .entry(format!("{:?}", m.material.emissive_source))
                .or_default() += 1;
            if m.material.is_pbr {
                pbr_true += 1;
            }
            if m.material.two_sided {
                two_sided += 1;
            }
            if m.material.is_decal {
                decal += 1;
            }
        }
    }

    println!("parsed nifs: {parsed}");
    println!("bsver hist: {bsver_hist:?}");
    println!(
        "PPLighting={pp} NoLighting={nl} Tile={tile} Sky={sky} TallGrass={grass} Water={water}"
    );
    println!("PP flags1 bit histogram:");
    for b in 0..32 {
        if f1_bits[b] > 0 {
            println!("  f1 bit {b:2}: {}", f1_bits[b]);
        }
    }
    println!("PP flags2 bit histogram:");
    for b in 0..32 {
        if f2_bits[b] > 0 {
            println!("  f2 bit {b:2}: {}", f2_bits[b]);
        }
    }
    println!("PP eye-env-only (bit17 w/o bit7|bit21): {eye_only}");
    println!("NoLighting decal-via-flags2-only: {nl_flags2_decal_only}");
    println!("shader_type hist (1000+ = NoLighting): {shader_type_hist:?}");
    let mut ems: Vec<_> = envmap_scale_hist.into_iter().collect();
    ems.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    println!("PP env_map_scale top: {:?}", &ems[..ems.len().min(10)]);
    println!("meshes={meshes} material_kind hist={kind_hist:?}");
    println!("emissive_source hist={emsrc_hist:?}");
    println!("is_pbr={pbr_true} two_sided={two_sided} decal={decal}");
}
