//! Throwaway (Oblivion audit dim5): NIFAL canonical PBR census over a BSA.
use byroredux_bsa::BsaArchive;
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;

fn main() {
    let path = std::env::args().nth(1).expect("usage: <bsa>");
    let archive = BsaArchive::open(&path).expect("open");
    let files: Vec<String> = archive
        .list_files()
        .into_iter()
        .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
        .map(|s| s.to_string())
        .collect();

    let mut meshes = 0u64;
    let mut rough: BTreeMap<String, u64> = Default::default();
    let mut metal: BTreeMap<String, u64> = Default::default();
    let mut gloss_scalar: BTreeMap<String, u64> = Default::default();
    let mut esrc: BTreeMap<String, u64> = Default::default();
    let mut none_esrc_samples: Vec<String> = vec![];
    let mut gloss_no_normal = 0u64; // gloss slot bound, no NIF normal slot
    let mut gloss_and_normal = 0u64;
    let mut no_normal_slot = 0u64;
    let mut nan_gloss = 0u64;
    let mut emis_nonzero = 0u64;
    let mut emis_nonzero_src_none = 0u64;
    let mut emmult: BTreeMap<String, u64> = Default::default();
    let mut kind: BTreeMap<u32, u64> = Default::default();

    for name in &files {
        let Ok(bytes) = archive.extract(name) else {
            continue;
        };
        let Ok(scene) = parse_nif(&bytes) else {
            continue;
        };
        let mut pool = byroredux_core::string::StringPool::new();
        let imported = byroredux_nif::import::import_nif_scene(&scene, &mut pool);
        for m in &imported.meshes {
            let mat = &m.material;
            meshes += 1;
            let r = mat.roughness_override.unwrap_or(f32::NAN);
            let mt = mat.metalness_override.unwrap_or(f32::NAN);
            *rough.entry(format!("{r:.2}")).or_insert(0) += 1;
            *metal.entry(format!("{mt:.2}")).or_insert(0) += 1;
            *gloss_scalar
                .entry(format!("{:.0}", mat.glossiness))
                .or_insert(0) += 1;
            if !mat.glossiness.is_finite() {
                nan_gloss += 1;
            }
            *esrc
                .entry(format!("{:?}", mat.emissive_source))
                .or_insert(0) += 1;
            *emmult
                .entry(format!("{:.2}", mat.emissive_mult))
                .or_insert(0) += 1;
            *kind.entry(mat.material_kind).or_insert(0) += 1;
            let has_gloss = mat.textures.smooth_spec.is_some();
            let has_normal = mat.textures.normal.is_some();
            if has_gloss && !has_normal {
                gloss_no_normal += 1;
            }
            if has_gloss && has_normal {
                gloss_and_normal += 1;
            }
            if !has_normal {
                no_normal_slot += 1;
            }
            let e = mat.emissive_color;
            if e[0] > 0.0 || e[1] > 0.0 || e[2] > 0.0 {
                emis_nonzero += 1;
                if format!("{:?}", mat.emissive_source) == "None" {
                    emis_nonzero_src_none += 1;
                    if none_esrc_samples.len() < 5 {
                        none_esrc_samples.push(name.clone());
                    }
                }
            }
        }
    }
    println!("files={} meshes={}", files.len(), meshes);
    println!("roughness_override: {rough:?}");
    println!("metalness_override: {metal:?}");
    println!("glossiness scalar: {gloss_scalar:?}");
    println!("non_finite_glossiness={nan_gloss}");
    println!("emissive_source: {esrc:?}");
    println!("emissive_mult: {emmult:?}");
    println!("material_kind: {kind:?}");
    println!("gloss_slot_without_nif_normal_slot={gloss_no_normal}");
    println!("gloss_slot_with_nif_normal_slot={gloss_and_normal}");
    println!("meshes_without_nif_normal_slot={no_normal_slot}");
    println!("emissive_nonzero={emis_nonzero} of_which_src_None={emis_nonzero_src_none}");
    println!("src_none_samples={none_esrc_samples:?}");
}
