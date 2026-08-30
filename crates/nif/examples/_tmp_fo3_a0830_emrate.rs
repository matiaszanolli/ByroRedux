//! TEMP scratch (audit 2026-08-30): which FO3 emitter-bearing NIFs yield no
//! authored spawn rate, and do they even carry an emitter controller?
use byroredux_bsa::BsaArchive;
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;

fn main() {
    let root = "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data";
    let archives = [
        "Fallout - Meshes.bsa",
        "Anchorage - Main.bsa",
        "BrokenSteel - Main.bsa",
        "PointLookout - Main.bsa",
        "ThePitt - Main.bsa",
        "Zeta - Main.bsa",
    ];
    let mut files_with_em = 0usize;
    let mut files_no_rate = 0usize;
    let mut no_rate_no_ctlr = 0usize;
    let mut no_rate_with_ctlr = 0usize;
    let mut examples: Vec<String> = Vec::new();
    let mut ctlr_block_hist: BTreeMap<String, usize> = BTreeMap::new();

    for arc_name in archives {
        let Ok(arc) = BsaArchive::open(&format!("{root}/{arc_name}")) else {
            continue;
        };
        for name in arc
            .list_files()
            .into_iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
        {
            let Ok(bytes) = arc.extract(name) else {
                continue;
            };
            let Ok(scene) = parse_nif(&bytes) else {
                continue;
            };
            let ems = byroredux_nif::import::import_nif_particle_emitters(&scene);
            if ems.is_empty() {
                continue;
            }
            files_with_em += 1;
            if ems.iter().any(|e| e.emitter_rate.is_some()) {
                continue;
            }
            files_no_rate += 1;
            let mut ctlrs: Vec<&str> = Vec::new();
            for b in scene.blocks.iter() {
                let t = b.block_type_name();
                if t.contains("EmitterCtlr") {
                    ctlrs.push(t);
                    *ctlr_block_hist.entry(t.to_string()).or_default() += 1;
                }
            }
            if ctlrs.is_empty() {
                no_rate_no_ctlr += 1;
            } else {
                no_rate_with_ctlr += 1;
                if examples.len() < 10 {
                    examples.push(format!("{name} ctlrs={ctlrs:?} emitters={}", ems.len()));
                }
            }
        }
    }
    println!("emitter-bearing files={files_with_em}, of which no scene rate={files_no_rate}");
    println!("  no rate + NO emitter controller = {no_rate_no_ctlr} (authored-absent)");
    println!("  no rate + HAS emitter controller = {no_rate_with_ctlr} (decode gap candidates)");
    println!("  controller block types on the gap set = {ctlr_block_hist:?}");
    for e in &examples {
        println!("    {e}");
    }
}
