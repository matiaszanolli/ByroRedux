//! unknown_types — enumerate NiUnknown type_names by count.
//!
//! Quick-and-dirty scan variant of `nif_stats` that surfaces which
//! block type names are hitting the `NiUnknown` fallback path in the
//! dispatch table.
use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::NiUnknown;
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn main() {
    let path = std::env::args().nth(1).expect("usage: unknown_types <bsa>");
    let path = PathBuf::from(path);
    let archive = BsaArchive::open(&path).expect("open BSA");
    let nif_files: Vec<String> = archive
        .list_files()
        .iter()
        .filter(|p| p.to_ascii_lowercase().ends_with(".nif"))
        .map(|s| s.to_string())
        .collect();
    eprintln!("scanning {} nifs from {:?}", nif_files.len(), path);

    let mut unk: BTreeMap<String, usize> = BTreeMap::new();
    for (i, nif) in nif_files.iter().enumerate() {
        if i > 0 && i % 1000 == 0 {
            eprintln!("  {}/{}", i, nif_files.len());
        }
        let Ok(bytes) = archive.extract(nif) else {
            continue;
        };
        let Ok(scene) = parse_nif(&bytes) else {
            continue;
        };
        for b in &scene.blocks {
            if let Some(u) = b.as_any().downcast_ref::<NiUnknown>() {
                *unk.entry(u.type_name.to_string()).or_insert(0) += 1;
            }
        }
    }

    let mut v: Vec<_> = unk.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    println!("─── NiUnknown type_name histogram ───");
    for (name, cnt) in v.iter().take(40) {
        println!("  {:>7}  {}", cnt, name);
    }
    println!("  ({} distinct unknown types)", v.len());
}
