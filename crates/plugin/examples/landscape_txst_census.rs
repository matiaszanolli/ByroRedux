//! Census the texture sets LTEX records bind to landscape layers: which
//! TXST slots vanilla terrain actually authors.
//!
//! Evidence harness for the terrain data-texture colour-space fix: LAND
//! resolves TX01 (normal) and TX07 (specular) per splat layer, and both used
//! to upload through the sRGB diffuse path. This measures how much shipped
//! terrain each role touches before and after that change.
//!
//! Usage:
//!   cargo run --release -p byroredux-plugin --example landscape_txst_census -- <ESM> [...]

use byroredux_plugin::esm;

fn main() -> anyhow::Result<()> {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path)?;
        let index = esm::records::parse_esm(&bytes)?;
        let sets = &index.cells.landscape_texture_sets;
        let count =
            |pick: fn(&esm::cell::TextureSet) -> bool| sets.values().filter(|s| pick(s)).count();
        println!("=== {path}");
        println!(
            "  LTEX texture sets {} / diffuse {} / normal {} / specular {} / height {} / env {}",
            sets.len(),
            count(|s| s.diffuse.is_some()),
            count(|s| s.normal.is_some()),
            count(|s| s.specular.is_some()),
            count(|s| s.height.is_some()),
            count(|s| s.env.is_some()),
        );
        let mut samples: Vec<_> = sets
            .iter()
            .filter(|(_, s)| s.normal.is_some() || s.specular.is_some())
            .collect();
        samples.sort_by_key(|(id, _)| **id);
        for (id, set) in samples.iter().take(6) {
            println!(
                "  LTEX {id:08X} normal={:?} specular={:?}",
                set.normal, set.specular
            );
        }
    }
    Ok(())
}
