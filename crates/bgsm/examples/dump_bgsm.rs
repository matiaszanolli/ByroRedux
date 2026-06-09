//! Extract a BGSM from a BA2 and dump its specular/smoothness/PBR fields.
//! Throwaway diagnostic for the FO4 "chrome concrete" investigation.
//!
//! Usage: dump_bgsm <Materials.ba2> <substring>

use byroredux_bgsm::{parse, MaterialFile};
use byroredux_bsa::Ba2Archive;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arch = Ba2Archive::open(&args[1]).expect("open BA2");
    let needle = args[2].to_lowercase();
    for f in arch.list_files() {
        if !f.to_lowercase().contains(&needle) || !f.to_lowercase().ends_with(".bgsm") {
            continue;
        }
        let bytes = match arch.extract(f) {
            Ok(b) => b,
            Err(e) => {
                println!("{f}: extract error {e}");
                continue;
            }
        };
        match parse(&bytes) {
            Ok(MaterialFile::Bgsm(m)) => {
                // Authored fields only — the spec-gloss → metal/rough
                // translation lives in `asset_provider::merge_bgsm_into_mesh`
                // (gated on `pbr`); don't duplicate it here or it drifts.
                let mx = m.specular_color[0]
                    .max(m.specular_color[1])
                    .max(m.specular_color[2]);
                let mn = m.specular_color[0]
                    .min(m.specular_color[1])
                    .min(m.specular_color[2]);
                let saturation = if mx > 1.0e-4 { (mx - mn) / mx } else { 0.0 };
                println!(
                    "{f}\n  v={} pbr={} spec_enabled={} spec_color={:?} spec_mult={:.3} smoothness={:.3}\n  spec_saturation={:.3} (legacy metalness signal)  template={:?}",
                    m.base.version, m.pbr, m.specular_enabled, m.specular_color, m.specular_mult,
                    m.smoothness, saturation, m.root_material_path
                );
            }
            Ok(_) => println!("{f}: BGEM"),
            Err(e) => println!("{f}: parse error {e}"),
        }
    }
}
