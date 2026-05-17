//! Substring-search helper — lists files in a BA2 whose path contains
//! a given case-insensitive needle. Built during the 2026-05-17 FO4
//! MedTek "missing materials" diagnosis to confirm which archive a
//! specific BGSM/texture actually lives in (e.g. is
//! `materials\setdressing\metallocker01.bgsm` in `Fallout4 -
//! Materials.ba2`? — yes).
//!
//! Usage:
//! ```bash
//! cargo run --release -p byroredux-bsa --example r5_check_ba2 \
//!     -- "/path/to/Fallout4 - Materials.ba2" "metallocker01"
//! ```
//!
//! Output is the first 20 matches sorted by archive order.
//!
//! Sibling to `r5_extract_pex_ba2.rs` (the R5 corpus extractor); both
//! stay around for future archive-coverage / path-normalisation
//! debugging.

use byroredux_bsa::Ba2Archive;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: r5_check_ba2 <ba2-archive> <needle>");
        std::process::exit(2);
    }
    let arch = Ba2Archive::open(&args[1]).expect("open BA2");
    let needle = args[2].to_lowercase();
    let mut hits = 0;
    for f in arch.list_files() {
        if f.to_lowercase().contains(&needle) {
            println!("  {}", f);
            hits += 1;
            if hits >= 20 {
                break;
            }
        }
    }
    eprintln!("{} hits matching '{}'", hits, needle);
}
