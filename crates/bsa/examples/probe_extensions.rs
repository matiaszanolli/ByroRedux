//! List the file-extension distribution inside a BSA. Throwaway
//! diagnostic for the M44 Phase 2 sound-BSA survey.

use byroredux_bsa::BsaArchive;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: probe_extensions <bsa>");
    let bsa = BsaArchive::open(&path).expect("open BSA");
    let files = bsa.list_files();
    println!("total files: {}", files.len());
    let mut ext_count = std::collections::BTreeMap::new();
    for f in &files {
        let ext = f.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        *ext_count.entry(ext).or_insert(0_usize) += 1;
    }
    for (e, c) in &ext_count {
        println!("  .{e}: {c}");
    }
    for (e, _) in &ext_count {
        let samples: Vec<_> = files
            .iter()
            .filter(|f| f.to_ascii_lowercase().ends_with(e.as_str()))
            .take(3)
            .collect();
        for s in samples {
            println!("  sample .{e}: {s}");
        }
    }
}
