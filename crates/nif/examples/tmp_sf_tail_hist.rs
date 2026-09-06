//! TEMPORARY audit scratch — Starfield opaque-tail length histogram (delete after use).
//!
//! `read_starfield_tail` swallows `block_size - consumed` into
//! `starfield_tail` for `BSLightingShaderProperty` / `BSEffectShaderProperty`
//! on `bsver >= STARFIELD`, which makes the parse-time drift histogram
//! structurally zero for those types (#2625). This measures the tail LENGTHS
//! instead — the number the drift metric can no longer report.

use byroredux_bsa::Ba2Archive;
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;

const ARCHIVES: &[&str] = &[
    "Starfield - Meshes01.ba2",
    "Starfield - MeshesPatch.ba2",
    "Starfield - FaceMeshes.ba2",
    "ShatteredSpace - Main01.ba2",
    "Starfield - LODMeshes.ba2",
];

fn main() {
    let base = std::env::var("BYROREDUX_STARFIELD_DATA").unwrap_or_else(|_| {
        "/mnt/data/SteamLibrary/steamapps/common/Starfield/Data".to_string()
    });
    let stride: usize = std::env::var("SF_STRIDE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1)
        .max(1);

    let mut lit: BTreeMap<usize, usize> = BTreeMap::new();
    let mut eff: BTreeMap<usize, usize> = BTreeMap::new();
    let mut lit_total = 0usize;
    let mut eff_total = 0usize;
    let mut files = 0usize;

    for name in ARCHIVES {
        let path = std::path::PathBuf::from(&base).join(name);
        let Ok(archive) = Ba2Archive::open(&path) else {
            eprintln!("[skip] {name}");
            continue;
        };
        let list: Vec<String> = archive
            .list_files()
            .into_iter()
            .filter(|f| byroredux_nif::corpus::is_nif_entry(f))
            .map(|s| s.to_string())
            .collect();
        let (mut a_lit, mut a_eff) = (0usize, 0usize);
        for (i, f) in list.iter().enumerate() {
            if i % stride != 0 {
                continue;
            }
            let Ok(data) = archive.extract(f) else { continue };
            let Ok(scene) = parse_nif(&data) else { continue };
            files += 1;
            for block in &scene.blocks {
                if let Some(b) = block
                    .as_any()
                    .downcast_ref::<byroredux_nif::blocks::shader::BSLightingShaderProperty>()
                {
                    *lit.entry(b.starfield_tail.len()).or_insert(0) += 1;
                    lit_total += 1;
                    a_lit += 1;
                } else if let Some(b) = block
                    .as_any()
                    .downcast_ref::<byroredux_nif::blocks::shader::BSEffectShaderProperty>()
                {
                    *eff.entry(b.starfield_tail.len()).or_insert(0) += 1;
                    eff_total += 1;
                    a_eff += 1;
                }
            }
        }
        println!("[{name}] lit={a_lit} eff={a_eff}");
    }

    println!("--- files={files} stride={stride} ---");
    println!("BSLightingShaderProperty tails: total={lit_total}");
    for (len, c) in &lit {
        println!("  LIT_TAIL len={len}\tcount={c}");
    }
    println!("BSEffectShaderProperty tails: total={eff_total}");
    for (len, c) in &eff {
        println!("  EFF_TAIL len={len}\tcount={c}");
    }
}
