//! List the file-extension distribution inside a BA2 archive.
//! Diagnostic counterpart to `probe_extensions.rs` (which only reads
//! BSA v103/v104/v105). Useful for any survey question that asks
//! "what file types ship in this archive" — originally written for
//! the Starfield `Materials.ba2` probe in #762 / SF-D6-03 (which
//! confirmed that the runtime materials archive ships exactly one
//! file: `materials\materialsbeta.cdb` — no loose `.mat`), kept
//! as a permanent BA2 utility for future surveys (FO76 textures,
//! Starfield textures01..NN, FO4 BA2 inventory, etc.).
//!
//! Usage: `cargo run --release -p byroredux-bsa --example ba2_extensions -- path/to.ba2`
use byroredux_bsa::Ba2Archive;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: ba2_extensions <ba2>");
    let ba2 = Ba2Archive::open(&path).expect("open BA2");
    let files = ba2.list_files();
    println!("total files: {}", files.len());
    let mut ext_count = std::collections::BTreeMap::new();
    for f in &files {
        let ext = f
            .rsplit('.')
            .next()
            .unwrap_or("(none)")
            .to_ascii_lowercase();
        *ext_count.entry(ext).or_insert(0_usize) += 1;
    }
    for (e, c) in &ext_count {
        println!("  .{e}: {c}");
    }
    println!("---");
    for (e, _) in &ext_count {
        let samples: Vec<_> = files
            .iter()
            .filter(|f| f.to_ascii_lowercase().ends_with(&format!(".{e}")))
            .take(2)
            .collect();
        for s in samples {
            println!("  sample .{e}: {s}");
        }
    }
}
