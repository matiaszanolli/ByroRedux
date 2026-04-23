//! locate_unknowns — enumerate which NIFs hold each NiUnknown type_name.
//!
//! Sibling to `unknown_types`: instead of just counting, it records the
//! first N (nif_path, block_index) tuples per failing type so you can
//! feed them directly into `trace_block` for bisect. Used by #554 Phase 1
//! (Oblivion 32-type NiUnknown pool investigation).
use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::NiUnknown;
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: locate_unknowns <bsa> [samples_per_type=3]");
        std::process::exit(1);
    }
    let path = PathBuf::from(&args[1]);
    let samples_per_type: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(3);

    let archive = BsaArchive::open(&path).expect("open BSA");
    let nif_files: Vec<String> = archive
        .list_files()
        .iter()
        .filter(|p| p.to_ascii_lowercase().ends_with(".nif"))
        .map(|s| s.to_string())
        .collect();
    eprintln!("scanning {} nifs from {:?}", nif_files.len(), path);

    let mut samples: BTreeMap<String, Vec<(String, usize)>> = BTreeMap::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for (i, nif) in nif_files.iter().enumerate() {
        if i > 0 && i % 1000 == 0 {
            eprintln!("  {}/{}", i, nif_files.len());
        }
        let Ok(bytes) = archive.extract(nif) else { continue };
        let Ok(scene) = parse_nif(&bytes) else { continue };
        for (idx, b) in scene.blocks.iter().enumerate() {
            if let Some(u) = b.as_any().downcast_ref::<NiUnknown>() {
                let key = u.type_name.to_string();
                *counts.entry(key.clone()).or_insert(0) += 1;
                let bucket = samples.entry(key).or_default();
                if bucket.len() < samples_per_type {
                    bucket.push((nif.clone(), idx));
                }
            }
        }
    }

    let mut ordered: Vec<_> = counts.iter().collect();
    ordered.sort_by(|a, b| b.1.cmp(a.1));
    println!("─── NiUnknown samples (first {} per type) ───", samples_per_type);
    for (name, cnt) in ordered {
        println!("\n{} × {}", cnt, name);
        if let Some(list) = samples.get(name) {
            for (nif, idx) in list {
                println!("  [{}] {}", idx, nif);
            }
        }
    }
}
