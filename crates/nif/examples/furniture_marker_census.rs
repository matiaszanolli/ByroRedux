//! Census of the legacy (Oblivion / FO3 / FNV) `BSFurnitureMarker` position refs in one
//! mesh archive: how many positions author each `position_ref_1` (the number of the
//! `furnituremarkerXX.nif` a position refers to), and, for the numbers above 14, which
//! NIFs author them. The source of the table in PRD `ai-packages-completion` AIP-SEAT-002.
//!
//! ```text
//! cargo run --release -p byroredux-nif --example furniture_marker_census -- <meshes.bsa>
//! ```

use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::extra_data::{BsFurnitureMarker, FurniturePositionData};
use std::collections::BTreeMap;

fn main() {
    let path = std::env::args().nth(1).expect("usage: furniture_marker_census <meshes.bsa>");
    let bsa = BsaArchive::open(&path).expect("open the archive");
    let mut histogram: BTreeMap<u8, usize> = BTreeMap::new();
    let mut examples: BTreeMap<u8, Vec<String>> = BTreeMap::new();
    let (mut nifs, mut with_positions) = (0, 0);
    for file in bsa.list_files() {
        if !byroredux_nif::corpus::is_nif_entry(file) {
            continue;
        }
        nifs += 1;
        let Ok(bytes) = bsa.extract(file) else { continue };
        let Ok(scene) = byroredux_nif::parse_nif(&bytes) else { continue };
        let mut had = false;
        for block in &scene.blocks {
            let Some(marker) = block.as_any().downcast_ref::<BsFurnitureMarker>() else { continue };
            for position in &marker.positions {
                if let FurniturePositionData::Legacy { orientation, position_ref_1, position_ref_2 } = position.data {
                    had = true;
                    *histogram.entry(position_ref_1).or_default() += 1;
                    let list = examples.entry(position_ref_1).or_default();
                    if list.len() < 5 {
                        list.push(format!("{file} (orient {orientation}, ref2 {position_ref_2}, z {:.0})", position.offset[2]));
                    }
                }
            }
        }
        if had {
            with_positions += 1;
        }
    }
    println!("{nifs} NIFs, {with_positions} with legacy furniture positions");
    for (number, count) in &histogram {
        println!("ref1={number:2}: {count} positions");
        if *number > 14 {
            for example in &examples[number] {
                println!("      {example}");
            }
        }
    }
}
