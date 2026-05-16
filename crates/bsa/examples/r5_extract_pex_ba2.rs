//! R5 Papyrus corpus extractor — pulls every `.pex` script out of
//! a BA2 archive into a flat output directory. Built for the
//! `docs/r5-evaluation.md` survey work and kept around for future
//! M47.2 transpiler corpus walks.
//!
//! Usage:
//! ```bash
//! cargo run --release -p byroredux-bsa --example r5_extract_pex_ba2 \
//!     -- "/path/to/Fallout4 - Misc.ba2" /tmp/fo4_pex/
//! ```
//!
//! Followed up by Champollion v1.3.2 via wine to decompile:
//! ```bash
//! WINEDEBUG=-all wine ~/.tools/Champollion.exe \
//!     -r -t -p Z:\\tmp\\fo4_psc Z:\\tmp\\fo4_pex
//! ```
//!
//! Output names are FLATTENED (the archive's path-separator
//! structure is discarded — only the file stem survives). Two
//! .pex with the same stem across different folders will collide
//! on the last writer; not a problem for any vanilla Bethesda
//! corpus surveyed to date but worth noting if a mod ever does it.

use byroredux_bsa::Ba2Archive;
use std::fs;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let ba2_path = &args[1];
    let out_dir = Path::new(&args[2]);
    let arch = Ba2Archive::open(ba2_path).expect("open BA2");
    let mut count = 0;
    for f in arch.list_files() {
        if f.to_ascii_lowercase().ends_with(".pex") {
            let data = arch.extract(&f).expect("extract");
            let stem = f.rsplit(|c| c == '\\' || c == '/').next().unwrap();
            fs::write(out_dir.join(stem), &data).expect("write");
            count += 1;
        }
    }
    eprintln!("extracted {} .pex from {}", count, ba2_path);
}
