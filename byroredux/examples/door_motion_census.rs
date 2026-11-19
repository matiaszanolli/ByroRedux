//! How the in-cell doors of one master move: the census behind PRD `door-runtime` DRT-REC-003.
//!
//! An *in-cell door* is a `DOOR` placement with no teleport in an interior cell. For each base
//! object placed that way the model is read from the mesh archives and classed by what it authors
//! (§3 of the PRD): `sequence` (a `NiControllerSequence`), `graph` (a `BSBehaviorGraphExtraData`
//! naming a behaviour graph), or `none` (the engine would swing it). Counts are given per base
//! object and per placement.
//!
//! ```text
//! cargo run --release -p byroredux --example door_motion_census -- <Master.esm> <meshes.bsa|.ba2>...
//! ```
//!
//! Set `DOOR_CENSUS_LIST=1` to print every base with its class, placements and model path, and
//! `DOOR_CENSUS_CELL=<cell editor id>` to print each in-cell door of that cell with its reference form
//! ID, base, class and position (engine Y-up: x, height, z is `x, z, -y` of the file's Z-up).
//!
//! Interior cells only, like the door census in the AI packages PRD §5.11. Archives are searched in
//! the order given; a base whose model is in none of them counts as `missing`, one whose NIF does
//! not parse as `unreadable`.

use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_plugin::RecordType;
use std::collections::{BTreeMap, HashMap};

enum Archive {
    Bsa(BsaArchive),
    Ba2(Ba2Archive),
}

impl Archive {
    fn extract(&self, path: &str) -> Option<Vec<u8>> {
        match self {
            Self::Bsa(a) => a.extract(path).ok(),
            Self::Ba2(a) => a.extract(path).ok(),
        }
    }
}

#[derive(Default, Clone, Copy)]
struct Tally {
    bases: u32,
    placements: u32,
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let esm = args
        .next()
        .expect("usage: door_motion_census <Master.esm> <meshes archive>...");
    let archives: Vec<Archive> = args
        .map(|path| {
            let lower = path.to_ascii_lowercase();
            if lower.ends_with(".ba2") {
                Archive::Ba2(Ba2Archive::open(&path).expect("open ba2"))
            } else {
                Archive::Bsa(BsaArchive::open(&path).expect("open bsa"))
            }
        })
        .collect();
    let index =
        byroredux_plugin::esm::parse_esm(&std::fs::read(&esm)?).map_err(|e| anyhow::anyhow!(e))?;

    let mut placements: HashMap<u32, u32> = HashMap::new();
    for cell in index.cells.cells.values() {
        for reference in &cell.references {
            let is_door = index
                .cells
                .statics
                .get(&reference.base_form_id)
                .is_some_and(|s| s.record_type == RecordType::DOOR);
            if is_door && reference.teleport.is_none() {
                *placements.entry(reference.base_form_id).or_default() += 1;
            }
        }
    }

    let mut classes: BTreeMap<&str, Tally> = BTreeMap::new();
    let mut class_of: HashMap<u32, &str> = HashMap::new();
    let mut sequence_names: BTreeMap<String, u32> = BTreeMap::new();
    for (&base, &count) in &placements {
        let model = &index.cells.statics[&base].model_path;
        let normalized = byroredux_bsa::normalize_path(&format!("meshes\\{model}"));
        let class = match archives.iter().find_map(|a| a.extract(&normalized)) {
            None => "missing",
            Some(bytes) => match byroredux_nif::parse_nif(&bytes) {
                Err(_) => "unreadable",
                Ok(scene) => {
                    let sequences = byroredux_nif::anim::import_kf(&scene);
                    if !sequences.is_empty() {
                        for clip in &sequences {
                            *sequence_names
                                .entry(clip.name.to_ascii_lowercase())
                                .or_default() += 1;
                        }
                        "sequence"
                    } else if byroredux_nif::import::extract_behavior_graph_file(&scene).is_some() {
                        "graph"
                    } else {
                        "none"
                    }
                }
            },
        };
        if std::env::var_os("DOOR_CENSUS_LIST").is_some() {
            println!("{class:<10} {base:08X} x{count:<4} {model}");
        }
        class_of.insert(base, class);
        let tally = classes.entry(class).or_default();
        tally.bases += 1;
        tally.placements += count;
    }

    if let Ok(cell_name) = std::env::var("DOOR_CENSUS_CELL") {
        if let Some(cell) = index.cells.cells.get(&cell_name.to_ascii_lowercase()) {
            for reference in &cell.references {
                if let Some(class) = class_of
                    .get(&reference.base_form_id)
                    .filter(|_| reference.teleport.is_none())
                {
                    let [x, y, z] = reference.position;
                    println!("{cell_name}: ref {:08X} base {:08X} {class} at ({x:.0}, {z:.0}, {:.0}) lock={:?}", reference.form_id, reference.base_form_id, -y, reference.lock);
                }
            }
        }
    }

    let total_bases: u32 = classes.values().map(|t| t.bases).sum();
    let total_placements: u32 = classes.values().map(|t| t.placements).sum();
    println!(
        "{esm}: {} in-cell door bases, {} placements ({:?})",
        total_bases, total_placements, index.game
    );
    println!("{:<12} {:>8} {:>11}", "class", "bases", "placements");
    for (class, tally) in &classes {
        println!("{class:<12} {:>8} {:>11}", tally.bases, tally.placements);
    }
    if !sequence_names.is_empty() {
        println!("sequence names (clips over the `sequence` bases):");
        for (name, count) in &sequence_names {
            println!("  {name:<24} {count}");
        }
    }
    Ok(())
}
