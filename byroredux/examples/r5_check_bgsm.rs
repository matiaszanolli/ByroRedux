//! BGSM inspection helper — extracts a BGSM file from a BA2 and prints
//! its diffuse / normal / smooth-spec texture paths plus its
//! `root_material_path` template parent. Built during the 2026-05-17
//! FO4 MedTek "missing materials" diagnosis to surface the
//! self-referential template chain in vanilla FO4 (e.g.
//! `defaulttemplate_wet.bgsm` whose `root_material_path` points back
//! at itself).
//!
//! Usage:
//! ```bash
//! cargo run --release -p byroredux --example r5_check_bgsm \
//!     -- "/path/to/Fallout4 - Materials.ba2" \
//!        "materials\\template\\defaulttemplate_wet.bgsm"
//! ```
//!
//! Note: BSA/BA2 path keys use backslashes; pass `\\` (Rust string
//! escape) so the shell delivers a literal backslash. Forward
//! slashes also work — the archive lookup normalises both.

use byroredux_bsa::Ba2Archive;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: r5_check_bgsm <ba2-archive> <bgsm-path>");
        std::process::exit(2);
    }
    let arch = Ba2Archive::open(&args[1]).expect("open BA2");
    let path = &args[2];
    let bytes = arch.extract(path).expect("extract");
    println!("Extracted {} bytes from '{}'", bytes.len(), path);
    let parsed = byroredux_bgsm::parse_bgsm(&bytes).expect("parse");
    println!("  version: {}", parsed.base.version);
    println!("  diffuse_texture: '{}'", parsed.diffuse_texture);
    println!("  normal_texture: '{}'", parsed.normal_texture);
    println!("  smooth_spec_texture: '{}'", parsed.smooth_spec_texture);
    println!("  root_material_path: {:?}", parsed.root_material_path);
}
