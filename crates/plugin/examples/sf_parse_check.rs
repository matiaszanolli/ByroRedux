//! Bridge test for Starfield ESM Phase 1 sub-step 3.
//!
//! Invokes the real [`parse_esm`] against an ESM file and reports
//! what the dispatch CAPTURED into `EsmIndex` — distinct from
//! `sf_smoke --recurse` which only counts what the walker SEES.
//!
//! Answers the key Phase 1 question: does the existing CELL handler
//! decode Starfield content correctly, or does it silently drop most
//! records on a subrecord-size drift?
//!
//! Usage:
//!   cargo run --release -p byroredux-plugin --example sf_parse_check -- <ESM_PATH>
//!
//! Output: text report comparing captured-vs-walker counts. Exit
//! status is always 0 (this is a measurement tool, not a gate).

use byroredux_plugin::esm::parse_esm;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    // Routes `log::warn!` from parse_esm (XCLL size-sanity, etc.) to stderr
    // when RUST_LOG is set. Without this, the dispatch warns are dropped
    // because `log` has no default subscriber.
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .try_init();

    let esm_path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: sf_parse_check ESM_PATH"))?;

    eprintln!("[sf_parse_check] {}", esm_path);
    let bytes = std::fs::read(&esm_path)?;
    let file_len = bytes.len();
    eprintln!("  file_size     : {} bytes", file_len);

    let t0 = Instant::now();
    let parse_result = parse_esm(&bytes);
    let elapsed = t0.elapsed();
    eprintln!("  parse time    : {:.3} s", elapsed.as_secs_f64());

    let index = match parse_result {
        Ok(idx) => {
            eprintln!("  parse status  : OK");
            idx
        }
        Err(e) => {
            eprintln!("  parse status  : ERROR — {}", e);
            eprintln!();
            eprintln!("  parse_esm() bailed out. Dispatch silently fell through somewhere.");
            return Ok(());
        }
    };

    eprintln!("  game_kind     : {:?}", index.game);
    eprintln!();
    eprintln!("  ── Captured into EsmIndex ──");
    let interior_cells = index.cells.cells.len();
    let mut interior_refrs = 0usize;
    let mut interior_max_refrs = 0usize;
    let mut interior_min_refrs = usize::MAX;
    for cell in index.cells.cells.values() {
        let n = cell.references.len();
        interior_refrs += n;
        interior_max_refrs = interior_max_refrs.max(n);
        if n > 0 {
            interior_min_refrs = interior_min_refrs.min(n);
        }
    }
    let interior_avg = if interior_cells > 0 {
        interior_refrs as f64 / interior_cells as f64
    } else {
        0.0
    };
    eprintln!("  interior cells: {}", interior_cells);
    eprintln!(
        "  interior REFRs: {} (avg {:.1}/cell, max {}, min {})",
        interior_refrs,
        interior_avg,
        interior_max_refrs,
        if interior_min_refrs == usize::MAX {
            0
        } else {
            interior_min_refrs
        },
    );

    let exterior_worldspaces = index.cells.exterior_cells.len();
    let mut exterior_cells_total = 0usize;
    let mut exterior_refrs_total = 0usize;
    for (_, grid) in index.cells.exterior_cells.iter() {
        for cell in grid.values() {
            exterior_cells_total += 1;
            exterior_refrs_total += cell.references.len();
        }
    }
    eprintln!(
        "  worldspaces   : {} (carrying {} exterior cells, {} REFRs)",
        exterior_worldspaces, exterior_cells_total, exterior_refrs_total,
    );

    let statics_count = index.cells.statics.len();
    let landscape_textures = index.cells.landscape_textures.len();
    let texture_sets = index.cells.texture_sets.len();
    eprintln!("  STAT-family   : {} base objects", statics_count);
    eprintln!(
        "  LTEX→TXST     : {} landscape textures",
        landscape_textures
    );
    eprintln!("  TXST records  : {} texture sets", texture_sets);

    let items = index.items.len();
    let containers = index.containers.len();
    let leveled_items = index.leveled_items.len();
    let leveled_npcs = index.leveled_npcs.len();
    let npcs = index.npcs.len();
    let races = index.races.len();
    let factions = index.factions.len();
    let globals = index.globals.len();
    eprintln!(
        "  items / containers / lvli / lvln : {} / {} / {} / {}",
        items, containers, leveled_items, leveled_npcs,
    );
    eprintln!(
        "  NPCs / races / factions / globals : {} / {} / {} / {}",
        npcs, races, factions, globals,
    );

    // Sample 3 interior cells if any captured — list EDIDs so a
    // follow-up can pick a Cydonia candidate by name.
    if interior_cells > 0 {
        let mut edids: Vec<&String> = index.cells.cells.keys().collect();
        edids.sort();
        eprintln!();
        eprintln!("  Sample interior cell EDIDs (first 5 alphabetical):");
        for edid in edids.iter().take(5) {
            if let Some(cell) = index.cells.cells.get(*edid) {
                eprintln!(
                    "    {} ({} REFRs, form 0x{:08X})",
                    edid,
                    cell.references.len(),
                    cell.form_id,
                );
            }
        }
        // Look for Cydonia-named cells specifically.
        let cydonia: Vec<&String> = edids
            .iter()
            .filter(|e| e.to_lowercase().contains("cydonia"))
            .copied()
            .collect();
        if !cydonia.is_empty() {
            eprintln!();
            eprintln!("  Cydonia matches ({}):", cydonia.len());
            for edid in cydonia.iter().take(10) {
                if let Some(cell) = index.cells.cells.get(*edid) {
                    eprintln!(
                        "    {} ({} REFRs, form 0x{:08X})",
                        edid,
                        cell.references.len(),
                        cell.form_id,
                    );
                }
            }
            if cydonia.len() > 10 {
                eprintln!("    ... +{} more", cydonia.len() - 10);
            }
        } else {
            eprintln!();
            eprintln!("  No Cydonia interior cells captured.");
        }
    }

    Ok(())
}
