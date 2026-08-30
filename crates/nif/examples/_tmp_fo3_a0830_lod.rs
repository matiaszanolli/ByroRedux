//! TEMP scratch (audit 2026-08-30): FO3 baked LOD asset inventory.
use byroredux_bsa::BsaArchive;
use std::collections::BTreeMap;
fn main() {
    let root = "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data";
    let mut per_world: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
    let mut total = 0usize;
    for a in [
        "Fallout - Meshes.bsa",
        "Fallout - Textures.bsa",
        "Anchorage - Main.bsa",
        "BrokenSteel - Main.bsa",
        "PointLookout - Main.bsa",
        "ThePitt - Main.bsa",
        "Zeta - Main.bsa",
    ] {
        let Ok(arc) = BsaArchive::open(&format!("{root}/{a}")) else {
            continue;
        };
        for f in arc.list_files() {
            let l = f.to_ascii_lowercase();
            if !l.contains("\\lod\\") && !l.starts_with("lod\\") {
                continue;
            }
            total += 1;
            // meshes\landscape\lod\<world>\... or textures\landscape\lod\<world>\...
            let parts: Vec<&str> = l.split('\\').collect();
            if let Some(i) = parts.iter().position(|p| *p == "lod") {
                if let Some(w) = parts.get(i + 1) {
                    let e = per_world.entry((*w).to_string()).or_default();
                    if l.ends_with(".nif") {
                        e.0 += 1;
                    } else if l.ends_with(".dds") {
                        e.1 += 1;
                    } else {
                        e.2 += 1;
                    }
                }
            }
        }
    }
    println!("total lod entries = {total}");
    for (w, (nif, dds, other)) in &per_world {
        println!("  {w:<28} nif={nif:<6} dds={dds:<6} other={other}");
    }
}
