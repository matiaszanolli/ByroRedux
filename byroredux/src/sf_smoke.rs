//! Starfield ESM smoke-test entry point (`--sf-smoke <CELL_EDID>`).
//!
//! Walks an ESM under the existing `GameKind::Starfield` dispatch path
//! (which today routes through the FO4 record parser) and reports two
//! things for a single named interior cell:
//!
//! 1. **Base-form resolve rate** — of N REFRs in the cell, how many
//!    point at a base form the parser actually decoded into
//!    `EsmCellIndex.statics`? This is the gate question for ROADMAP
//!    Milestone B (Starfield interior cell renders): a high rate means
//!    "FO4 dispatch handles the bulk; write the gaps", a low rate
//!    means "the schema diverged; write a real Starfield parser".
//! 2. **Per-record-type breakdown** — of the resolved REFRs, what 4-CC
//!    base-form types are represented (STAT, MSTT, FURN, LIGH, …)?
//!    The shape of this distribution tells us which Starfield-new
//!    record types (PNDT / STDT / BIOM / SFBK / SUNP / GBFM / GBFT)
//!    show up in the wild and need dedicated parsers.
//!
//! See #763 / SF-D6-04. The smoke is a planning-phase deliverable; it
//! prints to stdout, makes no engine state, and is gated on `--sf-smoke`
//! at the top of `main()`.

use anyhow::{Context, Result};
use byroredux_plugin::esm::cell::CellData;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::parse_esm;
use std::collections::HashMap;
use std::path::Path;

// `parse_esm` returns an `EsmIndex` that already carries the detected
// `game: GameKind` (set from the TES4 HEDR `Version` f32 via
// `GameKind::from_header`), so the smoke doesn't need to re-detect.

/// Smoke-test a single cell in an ESM under the current `GameKind`
/// dispatch. Reads `esm_path` from disk, parses it via the shared
/// `parse_esm` entry point, looks up `cell_edid` in the interior cell
/// index, and prints the resolve report.
///
/// Caller wires this from `main()` when `--sf-smoke <CELL>` is set;
/// `--esm <PATH>` provides the ESM path. The function never returns
/// `Ok(())` with a usable engine state — it's terminal: print, exit.
pub fn run(esm_path: &Path, cell_edid: &str) -> Result<()> {
    let bytes = std::fs::read(esm_path)
        .with_context(|| format!("failed to read ESM at {}", esm_path.display()))?;
    let size_mb = bytes.len() / (1024 * 1024);

    let index = parse_esm(&bytes).context("parse_esm failed")?;

    println!("─── ESM smoke ─────────────────────────────────────────────");
    println!("file       : {}", esm_path.display());
    println!("size       : {} MB", size_mb);
    println!("game kind  : {:?}", index.game);
    if index.game != GameKind::Starfield {
        println!(
            "note       : ESM is not Starfield. The smoke is most useful for SF \
             but works on any HEDR-detected game; resolve-rate below reflects \
             the active dispatch path, whatever that is."
        );
    }

    let target_key = cell_edid.to_ascii_lowercase();
    let Some(cell) = index.cells.cells.get(&target_key) else {
        println!("cell EDID  : {} (NOT FOUND in interior cell index)", cell_edid);
        println!(
            "─── available interiors (first 20) ──────────────────────────"
        );
        let mut keys: Vec<&String> = index.cells.cells.keys().collect();
        keys.sort();
        for k in keys.iter().take(20) {
            println!("  {}", k);
        }
        if index.cells.cells.len() > 20 {
            println!("  ... ({} more)", index.cells.cells.len() - 20);
        }
        return Err(anyhow::anyhow!(
            "cell EDID '{}' not in {} interior cells",
            cell_edid,
            index.cells.cells.len()
        ));
    };

    print_cell_report(cell, &index.cells);
    Ok(())
}

fn print_cell_report(
    cell: &CellData,
    cells_index: &byroredux_plugin::esm::cell::EsmCellIndex,
) {
    let total = cell.references.len();
    println!(
        "─── cell {} ───────────────────────────────────────────────",
        cell.editor_id
    );
    println!("display    : {:?}", cell.display_name.as_deref().unwrap_or("(no FULL)"));
    println!("interior   : {}", cell.is_interior);
    println!("references : {} REFRs", total);

    if total == 0 {
        println!("(cell has no REFRs — nothing to measure)");
        return;
    }

    // Tally resolved (base_form_id maps to a known StaticObject) vs not,
    // grouped by base record type when resolved. `statics` is the
    // primary base-form table; many record types route here (STAT,
    // MSTT, FURN, DOOR, LIGH, NPC_, ACTI, ALCH, AMMO, …). Unresolved
    // hits mean the base form was either (a) a record type the parser
    // doesn't dispatch yet, or (b) a Starfield-new record type with no
    // parser (PNDT / STDT / BIOM / SFBK / SUNP / GBFM / GBFT), or (c) a
    // FormID from a master plugin not loaded in this single-ESM smoke.
    let mut resolved_by_type: HashMap<String, usize> = HashMap::new();
    let mut unresolved_high_byte: HashMap<u8, usize> = HashMap::new();
    let mut unresolved_sample: Vec<u32> = Vec::new();
    let mut resolved = 0usize;
    for r in &cell.references {
        if let Some(obj) = cells_index.statics.get(&r.base_form_id) {
            *resolved_by_type.entry(obj.record_type.as_str().to_string()).or_default() += 1;
            resolved += 1;
        } else {
            // The high byte of a FormID is the master file slot (load
            // order index); for a single-ESM smoke any slot != 0 means
            // "form lives in an unloaded master" rather than "schema gap".
            let slot = (r.base_form_id >> 24) as u8;
            *unresolved_high_byte.entry(slot).or_default() += 1;
            if unresolved_sample.len() < 20 {
                unresolved_sample.push(r.base_form_id);
            }
        }
    }

    let pct = 100.0 * resolved as f32 / total as f32;
    println!("─── resolve rate ──────────────────────────────────────────");
    println!("resolved   : {} / {} ({:.1}%)", resolved, total, pct);
    println!("unresolved : {} / {} ({:.1}%)", total - resolved, total, 100.0 - pct);

    if !resolved_by_type.is_empty() {
        println!("─── resolved by base record type ──────────────────────────");
        let mut by_type: Vec<(String, usize)> = resolved_by_type.into_iter().collect();
        by_type.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        for (ty, count) in &by_type {
            let pct_t = 100.0 * *count as f32 / total as f32;
            println!("  {:>4}  {:>5}  ({:>5.1}%)", ty, count, pct_t);
        }
    }

    if !unresolved_high_byte.is_empty() {
        println!("─── unresolved by FormID master slot ──────────────────────");
        let mut by_slot: Vec<(u8, usize)> = unresolved_high_byte.into_iter().collect();
        by_slot.sort_by_key(|&(slot, _)| slot);
        for (slot, count) in by_slot {
            let hint = match slot {
                0x00 => "this ESM (parser gap — schema diverged or record type missing)",
                0xFD => "Medium Master (ESH) slot — load order index 0xFD",
                0xFE => "Light Master (ESL) slot",
                0xFF => "runtime / dynamic FormID",
                _ => "load-order slot (master not loaded in single-ESM smoke)",
            };
            println!("  slot 0x{:02X}  {:>5}  — {}", slot, count, hint);
        }
        println!("─── unresolved FormID sample (first {}) ────────────────────", unresolved_sample.len());
        for id in &unresolved_sample {
            println!("  {:08X}", id);
        }
    }

    println!("─── verdict ────────────────────────────────────────────────");
    if pct >= 80.0 {
        println!("high resolve rate — FO4 dispatch covers the bulk. Milestone B sized as");
        println!("\"fill the gaps\" rather than \"full rewrite\". Inspect the by-type table");
        println!("above for which records show up most; that's where SF-specific parsers");
        println!("(PNDT / STDT / BIOM / SFBK / SUNP / GBFM / GBFT and evolved STAT/CELL/");
        println!("REFR/LIGH/DOOR/MSTT/LGTM) deliver the most coverage.");
    } else if pct >= 50.0 {
        println!("partial resolve — FO4 dispatch picks up half. Milestone B requires a");
        println!("dedicated SF parser pass; the by-slot 0x00 unresolved count is the");
        println!("number of records inside Starfield.esm itself that the FO4 path");
        println!("doesn't decode.");
    } else {
        println!("low resolve rate — Starfield's schema has diverged enough from FO4");
        println!("that the dispatch route mostly drops base forms. Milestone B will");
        println!("need a `crates/plugin/src/legacy/starfield.rs` from-scratch parser,");
        println!("not a delta on FO4.");
    }
}
