//! TEMP (audit dim-6) — survey BSShaderCRC32 values on real Starfield NIFs.
use byroredux_bsa::Ba2Archive;
use byroredux_nif::blocks::shader::{BSEffectShaderProperty, BSLightingShaderProperty};
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn known() -> BTreeMap<u32, &'static str> {
    use byroredux_nif::shader_flags::bs_shader_crc32 as c;
    let mut m = BTreeMap::new();
    m.insert(c::DECAL, "DECAL");
    m.insert(c::DYNAMIC_DECAL, "DYNAMIC_DECAL");
    m.insert(c::TWO_SIDED, "TWO_SIDED");
    m.insert(c::CAST_SHADOWS, "CAST_SHADOWS");
    m.insert(c::ZBUFFER_TEST, "ZBUFFER_TEST");
    m.insert(c::ZBUFFER_WRITE, "ZBUFFER_WRITE");
    m.insert(c::VERTEX_COLORS, "VERTEX_COLORS");
    m.insert(c::PBR, "PBR");
    m.insert(c::SKINNED, "SKINNED");
    m.insert(c::ENVMAP, "ENVMAP");
    m.insert(c::VERTEX_ALPHA, "VERTEX_ALPHA");
    m.insert(c::FACE, "FACE");
    m.insert(c::GRAYSCALE_TO_PALETTE_COLOR, "GRAYSCALE_TO_PALETTE_COLOR");
    m.insert(c::HAIRTINT, "HAIRTINT");
    m.insert(c::SKIN_TINT, "SKIN_TINT");
    m.insert(c::EMIT_ENABLED, "EMIT_ENABLED");
    m.insert(c::GLOWMAP, "GLOWMAP");
    m.insert(c::REFRACTION, "REFRACTION");
    m.insert(c::REFRACTION_FALLOFF, "REFRACTION_FALLOFF");
    m.insert(c::NOFADE, "NOFADE");
    m.insert(c::INVERTED_FADE_PATTERN, "INVERTED_FADE_PATTERN");
    m.insert(c::RGB_FALLOFF, "RGB_FALLOFF");
    m.insert(c::EXTERNAL_EMITTANCE, "EXTERNAL_EMITTANCE");
    m.insert(c::MODELSPACENORMALS, "MODELSPACENORMALS");
    m.insert(c::TRANSFORM_CHANGED, "TRANSFORM_CHANGED");
    m.insert(c::EFFECT_LIGHTING, "EFFECT_LIGHTING");
    m.insert(c::FALLOFF, "FALLOFF");
    m.insert(c::SOFT_EFFECT, "SOFT_EFFECT");
    m.insert(c::GRAYSCALE_TO_PALETTE_ALPHA, "GRAYSCALE_TO_PALETTE_ALPHA");
    m.insert(c::WEAPON_BLOOD, "WEAPON_BLOOD");
    m.insert(c::LOD_OBJECTS, "LOD_OBJECTS");
    m.insert(c::NO_EXPOSURE, "NO_EXPOSURE");
    m
}

fn main() {
    let path = PathBuf::from(std::env::args().nth(1).expect("usage: <ba2>"));
    let limit: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);
    let archive = Ba2Archive::open(&path).expect("open BA2");
    let nifs: Vec<String> = archive
        .list_files()
        .iter()
        .filter(|p| p.to_ascii_lowercase().ends_with(".nif"))
        .map(|s| s.to_string())
        .take(limit)
        .collect();
    eprintln!("scanning {} nifs", nifs.len());

    let k = known();
    let mut hist: BTreeMap<u32, usize> = BTreeMap::new();
    let mut hist_esp: BTreeMap<u32, usize> = BTreeMap::new();
    let mut lsp_arrays: BTreeMap<String, usize> = BTreeMap::new();
    let mut esp_arrays: BTreeMap<String, usize> = BTreeMap::new();
    let mut lsp_numsf: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    let mut esp_numsf: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    let mut lsp_tail: BTreeMap<usize, usize> = BTreeMap::new();
    let mut esp_tail: BTreeMap<usize, usize> = BTreeMap::new();
    let mut n_lsp_full = 0usize;
    let mut n_lsp_stub = 0usize;
    let mut n_esp_full = 0usize;
    let mut n_esp_stub = 0usize;
    let mut lsp_with_crcs = 0usize;
    for (i, f) in nifs.iter().enumerate() {
        if i > 0 && i % 5000 == 0 {
            eprintln!("  {}/{}", i, nifs.len());
        }
        let Ok(bytes) = archive.extract(f) else {
            continue;
        };
        let Ok(scene) = parse_nif(&bytes) else {
            continue;
        };
        for b in &scene.blocks {
            if let Some(p) = b.as_any().downcast_ref::<BSLightingShaderProperty>() {
                if p.material_reference {
                    n_lsp_stub += 1;
                    continue;
                }
                n_lsp_full += 1;
                if !p.sf1_crcs.is_empty() || !p.sf2_crcs.is_empty() {
                    lsp_with_crcs += 1;
                }
                *lsp_numsf
                    .entry((p.sf1_crcs.len(), p.sf2_crcs.len()))
                    .or_insert(0) += 1;
                *lsp_arrays
                    .entry(format!("{:?}|{:?}", p.sf1_crcs, p.sf2_crcs))
                    .or_insert(0usize) += 1;
                *lsp_tail.entry(p.starfield_tail.len()).or_insert(0) += 1;
                for c in p.sf1_crcs.iter().chain(p.sf2_crcs.iter()) {
                    *hist.entry(*c).or_insert(0) += 1;
                }
            } else if let Some(p) = b.as_any().downcast_ref::<BSEffectShaderProperty>() {
                if p.material_reference {
                    n_esp_stub += 1;
                    continue;
                }
                n_esp_full += 1;
                *esp_numsf
                    .entry((p.sf1_crcs.len(), p.sf2_crcs.len()))
                    .or_insert(0) += 1;
                *esp_arrays
                    .entry(format!("{:?}|{:?}", p.sf1_crcs, p.sf2_crcs))
                    .or_insert(0usize) += 1;
                *esp_tail.entry(p.starfield_tail.len()).or_insert(0) += 1;
                for c in p.sf1_crcs.iter().chain(p.sf2_crcs.iter()) {
                    *hist_esp.entry(*c).or_insert(0) += 1;
                }
            }
        }
    }
    let mut la: Vec<_> = lsp_arrays.into_iter().collect();
    la.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    println!("--- LSP distinct (sf1|sf2) arrays, top 20 ---");
    for (a, n) in la.iter().take(20) {
        println!("  {:>7}  {}", n, a);
    }
    let mut ea: Vec<_> = esp_arrays.into_iter().collect();
    ea.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    println!("--- ESP distinct (sf1|sf2) arrays, top 20 ---");
    for (a, n) in ea.iter().take(20) {
        println!("  {:>7}  {}", n, a);
    }
    println!("LSP (num_sf1,num_sf2) hist: {:?}", lsp_numsf);
    println!("ESP (num_sf1,num_sf2) hist: {:?}", esp_numsf);
    println!("LSP tail-len hist: {:?}", lsp_tail);
    println!("ESP tail-len hist: {:?}", esp_tail);
    println!("--- ESP-only CRC histogram ---");
    let mut ve: Vec<_> = hist_esp.clone().into_iter().collect();
    ve.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    for (c, n) in &ve {
        println!(
            "  {:>9}  {:>10}  {}",
            n,
            c,
            k.get(c).copied().unwrap_or("<UNKNOWN>")
        );
    }
    println!("--- LSP-only CRC histogram ---");
    println!(
        "BSLightingShaderProperty: full={} stub={}  (full with any CRC={})",
        n_lsp_full, n_lsp_stub, lsp_with_crcs
    );
    println!(
        "BSEffectShaderProperty:   full={} stub={}",
        n_esp_full, n_esp_stub
    );
    let mut v: Vec<_> = hist.into_iter().collect();
    v.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    let mut unknown_total = 0usize;
    let mut known_total = 0usize;
    println!("--- CRC histogram ({} distinct) ---", v.len());
    for (c, n) in &v {
        match k.get(c) {
            Some(name) => {
                known_total += n;
                println!("  {:>9}  {:>10}  {}", n, c, name);
            }
            None => {
                unknown_total += n;
                println!("  {:>9}  {:>10}  <UNKNOWN>", n, c);
            }
        }
    }
    println!(
        "totals: known={} unknown={} ({} distinct unknown values)",
        known_total,
        unknown_total,
        v.iter().filter(|(c, _)| !k.contains_key(c)).count()
    );
}
