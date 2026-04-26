//! full_histogram — like nif_stats but dumps every block type seen
//! (no top-20 cap) as TSV: `<count>\t<type>`.
//!
//! Usage: cargo run -p byroredux-nif --example full_histogram -- <archive>

use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn process_bytes(hist: &mut BTreeMap<String, usize>, bytes: &[u8]) {
    if let Ok(scene) = parse_nif(bytes) {
        for b in &scene.blocks {
            *hist.entry(b.block_type_name().to_string()).or_insert(0) += 1;
        }
    }
}

fn process_bsa(hist: &mut BTreeMap<String, usize>, path: &Path) -> Result<(), String> {
    let archive = BsaArchive::open(path).map_err(|e| format!("open BSA: {e}"))?;
    let nif_files: Vec<String> = archive
        .list_files()
        .iter()
        .filter(|p| p.to_ascii_lowercase().ends_with(".nif"))
        .map(|s| s.to_string())
        .collect();
    eprintln!("BSA {} -> {} NIFs", path.display(), nif_files.len());
    for nif_path in &nif_files {
        if let Ok(bytes) = archive.extract(nif_path) {
            process_bytes(hist, &bytes);
        }
    }
    Ok(())
}

fn process_ba2(hist: &mut BTreeMap<String, usize>, path: &Path) -> Result<(), String> {
    let archive = Ba2Archive::open(path).map_err(|e| format!("open BA2: {e}"))?;
    let nif_files: Vec<String> = archive
        .list_files()
        .iter()
        .filter(|p| p.to_ascii_lowercase().ends_with(".nif"))
        .map(|s| s.to_string())
        .collect();
    eprintln!("BA2 {} -> {} NIFs", path.display(), nif_files.len());
    for nif_path in &nif_files {
        if let Ok(bytes) = archive.extract(nif_path) {
            process_bytes(hist, &bytes);
        }
    }
    Ok(())
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path_arg) = args.next() else {
        eprintln!("usage: full_histogram <archive>");
        std::process::exit(2);
    };
    let path = PathBuf::from(path_arg);

    let mut hist: BTreeMap<String, usize> = BTreeMap::new();

    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    let r = match ext.as_str() {
        "bsa" => process_bsa(&mut hist, &path),
        "ba2" => process_ba2(&mut hist, &path),
        _ => {
            eprintln!("expected .bsa or .ba2");
            std::process::exit(2);
        }
    };
    if let Err(e) = r {
        eprintln!("error: {e}");
        std::process::exit(2);
    }

    let mut sorted: Vec<(&String, &usize)> = hist.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));
    let total: usize = hist.values().sum();
    println!(
        "# full_histogram: {} blocks across {} distinct types",
        total,
        sorted.len()
    );
    for (name, count) in sorted {
        println!("{}\t{}", count, name);
    }
}
