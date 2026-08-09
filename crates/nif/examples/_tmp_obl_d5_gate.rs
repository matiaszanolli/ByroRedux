//! Throwaway (Oblivion audit dim5): normal-alpha-as-spec gate outcome census.
use byroredux_bsa::BsaArchive;
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;

fn main() {
    let mesh_bsa = std::env::args()
        .nth(1)
        .expect("usage: <meshes bsa> <textures bsa>");
    let tex_bsa = std::env::args()
        .nth(2)
        .expect("usage: <meshes bsa> <textures bsa>");
    let tex = BsaArchive::open(&tex_bsa).expect("open textures");
    // path -> has_alpha (DXT3/DXT5/uncompressed-with-alpha)
    let mut alpha_map: BTreeMap<String, bool> = Default::default();
    for f in tex.list_files() {
        let lf = f.to_ascii_lowercase();
        if !lf.ends_with(".dds") {
            continue;
        }
        alpha_map.insert(lf.replace('/', "\\"), false);
    }
    let archive = BsaArchive::open(&mesh_bsa).expect("open meshes");
    let files: Vec<String> = archive
        .list_files()
        .into_iter()
        .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
        .map(|s| s.to_string())
        .collect();

    let mut total = 0u64;
    let mut gate_pass_pre_alpha = 0u64; // kind/metal/env/normal-exists/no-gloss
    let mut gate_pass_with_alpha = 0u64; // + normal DDS actually has alpha
    let mut rough_hist: BTreeMap<String, u64> = Default::default();
    let mut classifier_hist: BTreeMap<String, u64> = Default::default();
    let mut mirror_samples: Vec<String> = vec![];
    let mut normal_missing = 0u64;

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
            total += 1;
            let metal = mat.metalness_override.unwrap_or(0.0);
            let classifier_rough = mat.roughness_override.unwrap_or(f32::NAN);
            if mat.material_kind >= 100 {
                continue;
            }
            if metal >= 0.3 {
                continue;
            }
            if mat.env_map_scale > 0.3 {
                continue;
            }
            if mat.textures.smooth_spec.is_some() {
                continue;
            }
            // derive normal path the way spawn.rs does
            let base = mat
                .textures
                .base_color
                .and_then(|s| pool.resolve(s))
                .map(|s| s.to_string());
            let normal_path = match mat.textures.normal.and_then(|s| pool.resolve(s)) {
                Some(p) => p.to_string(),
                None => match &base {
                    Some(b) => match b.rfind('.') {
                        Some(d) => format!("{}_n{}", &b[..d], &b[d..]),
                        None => format!("{b}_n.dds"),
                    },
                    None => continue,
                },
            };
            let key = normal_path.to_ascii_lowercase().replace('/', "\\");
            let key = if key.starts_with("textures\\") {
                key
            } else {
                format!("textures\\{key}")
            };
            if !alpha_map.contains_key(&key) {
                normal_missing += 1;
                continue;
            }
            gate_pass_pre_alpha += 1;
            let Ok(nb) = tex.extract(&key) else { continue };
            if nb.len() < 92 {
                continue;
            }
            let flags = u32::from_le_bytes([nb[80], nb[81], nb[82], nb[83]]);
            let cc = String::from_utf8_lossy(&nb[84..88]).to_string();
            let has_alpha = (flags & 0x4 != 0) && (cc == "DXT3" || cc == "DXT5");
            if !has_alpha {
                continue;
            }
            gate_pass_with_alpha += 1;
            let g = mat.glossiness;
            let r = (1.0f32 - g / 100.0).clamp(0.05, 0.95);
            *rough_hist.entry(format!("{r:.2}")).or_insert(0) += 1;
            *classifier_hist
                .entry(format!("{classifier_rough:.2}"))
                .or_insert(0) += 1;
            if r <= 0.10 && mirror_samples.len() < 8 {
                mirror_samples.push(format!("{name} g={g}"));
            }
        }
    }
    println!("total_meshes={total}");
    println!("normal_sibling_missing_in_textures_bsa={normal_missing}");
    println!("gate_pass_before_alpha_check={gate_pass_pre_alpha}");
    println!("gate_pass_with_alpha_normal={gate_pass_with_alpha}");
    println!("resulting_roughness_hist={rough_hist:?}");
    println!("classifier_roughness_that_got_overridden={classifier_hist:?}");
    println!("mirror_samples={mirror_samples:?}");
}
