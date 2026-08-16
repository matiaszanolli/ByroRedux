//! TEMP scratch (audit 2026-08-16): Skyrim SE BSXFlags bit-5 blast radius.
//! Counts pre-FO4 NIFs with BSXFlags bit 5 set and how many carry real
//! geometry (i.e. would be wholesale-dropped by the cell loader).
use byroredux_bsa::BsaArchive;
use byroredux_core::string::StringPool;
use byroredux_nif::import::{extract_bsx_flags, import_nif_with_collision};
use byroredux_nif::parse_nif;

fn main() {
    let mut total = 0usize;
    let mut bit5 = 0usize;
    let mut bit5_pure_marker = 0usize;
    let mut bit5_real = 0usize;
    let mut samples: Vec<(String, usize, usize, usize, u32)> = Vec::new();

    for path in std::env::args().skip(1) {
        let Ok(arc) = BsaArchive::open(&path) else {
            eprintln!("open fail {path}");
            continue;
        };
        let names: Vec<String> = arc
            .list_files()
            .into_iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
            .map(|s| s.to_string())
            .collect();
        eprintln!("{path}: {} nifs", names.len());
        for name in &names {
            let Ok(bytes) = arc.extract(name) else {
                continue;
            };
            let Ok(scene) = parse_nif(&bytes) else {
                continue;
            };
            total += 1;
            let bsx = extract_bsx_flags(&scene);
            if bsx & 0x20 == 0 {
                continue;
            }
            if scene.bsver >= 130 {
                continue;
            }
            bit5 += 1;
            let mut pool = StringPool::new();
            let (meshes, cols) = import_nif_with_collision(&scene, &mut pool);
            let tris: usize = meshes.iter().map(|m| m.indices.len() / 3).sum();
            if meshes.is_empty() && cols.is_empty() {
                bit5_pure_marker += 1;
            } else {
                bit5_real += 1;
                if samples.len() < 4000 {
                    samples.push((name.clone(), meshes.len(), tris, cols.len(), bsx));
                }
            }
        }
    }
    println!("total nifs parsed:              {total}");
    println!("pre-FO4 with BSX bit5:          {bit5}");
    println!("  pure markers (0 mesh 0 coll): {bit5_pure_marker}");
    println!("  REAL geometry (DROPPED):      {bit5_real}");
    samples.sort_by_key(|s| std::cmp::Reverse(s.2));
    for (n, m, t, c, b) in samples.iter().take(25) {
        println!("  {n}  meshes={m} tris={t} colliders={c} bsx=0x{b:X}");
    }
}
