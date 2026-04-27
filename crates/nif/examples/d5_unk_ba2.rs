//! D5 audit helper: enumerate NiUnknown types from BSA or BA2 archive.
use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_nif::blocks::NiUnknown;
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;
use std::path::PathBuf;

enum Arc {
    Bsa(BsaArchive),
    Ba2(Ba2Archive),
}

impl Arc {
    fn list(&self) -> Vec<String> {
        match self {
            Arc::Bsa(a) => a.list_files().iter().map(|s| s.to_string()).collect(),
            Arc::Ba2(a) => a.list_files().iter().map(|s| s.to_string()).collect(),
        }
    }
    fn extract(&self, p: &str) -> std::io::Result<Vec<u8>> {
        match self {
            Arc::Bsa(a) => a.extract(p),
            Arc::Ba2(a) => a.extract(p),
        }
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: unk_ba2 <archive>");
    let path = PathBuf::from(path);
    let arc = if BsaArchive::open(&path).is_ok() {
        Arc::Bsa(BsaArchive::open(&path).unwrap())
    } else {
        Arc::Ba2(Ba2Archive::open(&path).expect("open BA2"))
    };
    let files = arc.list();
    let nifs: Vec<&String> = files.iter().filter(|p| p.to_ascii_lowercase().ends_with(".nif")).collect();
    eprintln!("scanning {} nifs from {:?}", nifs.len(), path);
    let mut unk: BTreeMap<String, usize> = BTreeMap::new();
    let mut total_blocks: usize = 0;
    let mut parsed_files: usize = 0;
    let mut failed_files: usize = 0;
    for (i, nif) in nifs.iter().enumerate() {
        if i > 0 && i % 2000 == 0 { eprintln!("  {}/{}", i, nifs.len()); }
        let Ok(bytes) = arc.extract(nif) else { failed_files += 1; continue; };
        let Ok(scene) = parse_nif(&bytes) else { failed_files += 1; continue; };
        parsed_files += 1;
        for b in &scene.blocks {
            total_blocks += 1;
            if let Some(u) = b.as_any().downcast_ref::<NiUnknown>() {
                *unk.entry(u.type_name.to_string()).or_insert(0) += 1;
            }
        }
    }
    let mut v: Vec<_> = unk.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    println!("─── NiUnknown type_name histogram ({}) ───", path.file_name().unwrap().to_string_lossy());
    println!("  parsed_files={} failed_files={} total_blocks={}", parsed_files, failed_files, total_blocks);
    let unk_total: usize = v.iter().map(|x| x.1).sum();
    println!("  unknown_block_total={} ({} distinct types)", unk_total, v.len());
    for (name, cnt) in v.iter().take(60) {
        println!("  {:>7}  {}", cnt, name);
    }
}
