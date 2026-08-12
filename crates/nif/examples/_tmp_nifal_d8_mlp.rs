//! Throwaway (NIFAL audit 2026-08-12, dim 8): which BSShaderTextureSet slot
//! actually carries the MultiLayerParallax inner layer on shipped content?
//! nif.xml's BSLightingShaderType enum says "Layer(TS7)"; its
//! BSShaderTextureSet field doc says slot 6 = "Subsurface for Multilayer
//! Parallax", slot 7 = "Back Lighting Map". The importer and the REFR
//! overlay disagree accordingly, so measure.
use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::shader::{BSLightingShaderProperty, BSShaderTextureSet};
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;

fn main() {
    let path = std::env::args().nth(1).expect("usage: <bsa>");
    let want: u32 = std::env::args()
        .nth(2)
        .map(|s| s.parse().unwrap())
        .unwrap_or(11);
    let archive = BsaArchive::open(&path).expect("open");
    let files: Vec<String> = archive
        .list_files()
        .into_iter()
        .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
        .map(|s| s.to_string())
        .collect();

    // slot index -> count of non-empty, plus a few sample paths
    let mut slot_counts: BTreeMap<usize, u64> = Default::default();
    let mut slot_samples: BTreeMap<usize, Vec<String>> = Default::default();
    let mut slot_all: BTreeMap<usize, Vec<String>> = Default::default();
    let mut shapes = 0u64;
    let mut nifs = 0u64;

    for name in &files {
        let Ok(bytes) = archive.extract(name) else {
            continue;
        };
        let Ok(scene) = parse_nif(&bytes) else {
            continue;
        };
        let mut hit = false;
        for i in 0..scene.len() {
            let Some(shader) = scene.get_as::<BSLightingShaderProperty>(i) else {
                continue;
            };
            if shader.shader_type != want {
                continue;
            }
            shapes += 1;
            hit = true;
            let Some(ts_idx) = shader.texture_set_ref.index() else {
                continue;
            };
            let Some(ts) = scene.get_as::<BSShaderTextureSet>(ts_idx) else {
                continue;
            };
            for (s, tex) in ts.textures.iter().enumerate() {
                if tex.trim().is_empty() {
                    continue;
                }
                *slot_counts.entry(s).or_insert(0) += 1;
                slot_all.entry(s).or_default().push(tex.clone());
                let e = slot_samples.entry(s).or_default();
                if e.len() < 4 {
                    e.push(tex.clone());
                }
            }
        }
        if hit {
            nifs += 1;
        }
    }

    println!("shader_type={want}: {shapes} properties across {nifs} NIFs");
    for (slot, count) in &slot_counts {
        println!("  slot {slot}: {count} non-empty");
        let mut suffix: BTreeMap<String, u64> = Default::default();
        for s in slot_all.get(slot).unwrap_or(&Vec::new()) {
            let stem = s
                .rsplit(['\\', '/'])
                .next()
                .unwrap_or(s)
                .rsplit_once('.')
                .map(|(a, _)| a)
                .unwrap_or(s)
                .to_ascii_lowercase();
            let tail = match stem.rsplit_once('_') {
                Some((_, t)) if t.len() <= 3 => format!("_{t}"),
                _ => "(none)".to_string(),
            };
            *suffix.entry(tail).or_insert(0) += 1;
        }
        println!("      suffixes: {suffix:?}");
        for s in slot_samples.get(slot).unwrap_or(&Vec::new()) {
            println!("      {s}");
        }
    }
}
